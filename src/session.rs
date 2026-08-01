use chrono::Local;
use serenity::all::Context;
use serenity::async_trait;
use serenity::model::id::{ChannelId, GuildId, UserId};
use songbird::events::{Event, EventContext, EventHandler as VoiceEventHandler};
use songbird::CoreEvent;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::track::UserTrack;

/// 一回の録音セッションを管理する構造体
pub struct RecordingSession {
    /// 出力ディレクトリのパス
    dir_path: PathBuf,
    /// SSRCからユーザーIDへのマップ
    ssrc_to_user: HashMap<u32, UserId>,
    /// ユーザーIDからSSRCへの逆引きマップ
    user_id_to_ssrc: HashMap<UserId, u32>,
    /// ユーザーIDから表示名へのマップ
    user_id_to_name: HashMap<UserId, String>,
    /// ユーザーごとの音声トラック
    tracks: HashMap<UserId, UserTrack>,
    /// 経過Tick数（20ms単位）
    tick_count: u64,
    /// Discord チャンネルのビットレート
    bitrate: u32,
    /// 録音対象のギルドID
    guild_id: GuildId,
    /// Songbird のマネージャー
    manager: Arc<songbird::Songbird>,
}
impl RecordingSession {
    /// ボイスチャンネルに接続し、録音セッションを開始します。
    pub async fn start(
        guild_id: GuildId,
        channel_id: ChannelId,
        ctx: &Context,
        session: &Arc<Mutex<Option<RecordingSession>>>,
        output_dir: &PathBuf,
    ) {
        let manager = songbird::get(ctx).await.expect("Songbirdの初期化に失敗").clone();
        let dir_name = Local::now().format("%Y%m%d%H%M%S").to_string();
        {
            let mut guard = session.lock().await;
            if guard.is_some() {
                return;
            }

            let bitrate = channel_id.to_channel(&ctx.http).await
                .ok()
                .and_then(|c| c.guild())
                .and_then(|gc| gc.bitrate)
                .unwrap_or(64000);

            let dir_path = output_dir.join(dir_name.clone());
            fs::create_dir_all(&dir_path).unwrap();

            *guard = Some(Self {
                dir_path,
                ssrc_to_user: HashMap::new(),
                user_id_to_ssrc: HashMap::new(),
                user_id_to_name: HashMap::new(),
                tracks: HashMap::new(),
                tick_count: 0,
                bitrate,
                guild_id,
                manager: manager.clone(),
            })
        }

        let call = manager.get_or_insert(guild_id);
        {
            let mut handler = call.lock().await;
            handler.add_global_event(Event::Core(CoreEvent::SpeakingStateUpdate), Receiver {
                session: session.clone(),
                ctx: ctx.clone(),
            });
            handler.add_global_event(Event::Core(CoreEvent::VoiceTick), Receiver {
                session: session.clone(),
                ctx: ctx.clone(),
            });
        }
        match manager.join(guild_id, channel_id).await {
            Ok(call) => {
                let mut handler = call.lock().await;
                let _ = handler.mute(true).await;
            }
            Err(e) => {
                eprintln!("音声チャンネルへの参加に失敗: {:?}", e);
            }
        }
        println!("[{}] 録音セッションを開始しました。", dir_name);
    }

    /// Opus ファイルを正常に終了させ、ボイスチャンネルから切断します。
    pub async fn end(mut self) {
        self.finalize_tracks();

        if let Some(call) = self.manager.get(self.guild_id) {
            let mut handler = call.lock().await;
            let _ = handler.mute(false).await;
        }
        let _ = self.manager.remove(self.guild_id).await;
    }

    /// Opus ファイルを確定させます。
    fn finalize_tracks(&mut self) {
        for (_, track) in self.tracks.drain() {
            if let Err(e) = track.finalize() {
                eprintln!("ファイルの確定に失敗しました: {}", e);
            }
        }
    }
}
impl Drop for RecordingSession {
    /// `end()` 呼び忘れ時のセーフティネットとして、Opus ファイルの確定のみ行います。
    ///
    /// Drop への依存は意図していません。**正常系では、必ず `end()` を明示的に呼び出してください。**。
    /// この実装は、同期 fn の制約により、非同期処理であるボイスチャンネルからの離脱を行えません。
    /// また `end()` が `self` を消費するため、正常経路では `tracks` が空の状態で呼ばれ実質ノーオペとなります。
    fn drop(&mut self) {
        self.finalize_tracks();
    }
}

/// Songbirdの音声イベントを受信するハンドラ
struct Receiver {
    /// 共有セッションへの参照
    session: Arc<Mutex<Option<RecordingSession>>>,
    /// Serenityコンテキスト（ユーザー名解決に使用）
    ctx: Context,
}
#[async_trait]
impl VoiceEventHandler for Receiver {
    /// 音声イベントを処理します。
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        let mut session_opt = self.session.lock().await;
        let Some(session) = session_opt.as_mut() else { return None; };

        match ctx {
            EventContext::SpeakingStateUpdate(speaking) => {
                let Some(voice_uid) = speaking.user_id else { return None; };
                let id = UserId::new(voice_uid.0);
                session.ssrc_to_user.insert(speaking.ssrc, id);
                session.user_id_to_ssrc.insert(id, speaking.ssrc);

                if !session.tracks.contains_key(&id) {
                    let name = match id.to_user(&self.ctx.http).await {
                        Ok(user) => user.name,
                        Err(_) => id.to_string(),
                    };
                    session.user_id_to_name.insert(id, name.clone());

                    let file_path = session.dir_path.join(format!("{}.opus", name));
                    let mut track = UserTrack::create(file_path, session.bitrate)
                        .map_err(|e| eprintln!("Opusファイル作成失敗: {}", e))
                        .ok()?;

                    let missing_frames = session.tick_count;
                    for _ in 0..missing_frames {
                        if let Err(e) = track.write_silent_frame() {
                            eprintln!("無音書き込み失敗: {}", e);
                            break;
                        }
                    }

                    session.tracks.insert(id, track);
                }
            }
            EventContext::VoiceTick(tick) => {
                session.tick_count += 1;

                for (user_id, track) in session.tracks.iter_mut() {
                    let ssrc = session.user_id_to_ssrc.get(user_id).copied();

                    let mut audio_written = false;
                    if let Some(ssrc) = ssrc {
                        if let Some(voice_data) = tick.speaking.get(&ssrc) {
                            if let Some(decoded) = &voice_data.decoded_voice {
                                if let Err(e) = track.write_frame(decoded) {
                                    eprintln!("音声書き込み失敗: {}", e);
                                }
                                audio_written = true;
                            }
                        }
                    }

                    if !audio_written {
                        if let Err(e) = track.write_silent_frame() {
                            eprintln!("無音書き込み失敗: {}", e);
                        }
                    }
                }
            }
            _ => {}
        }
        None
    }
}
