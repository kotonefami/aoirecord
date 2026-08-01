use serenity::all::{Context, EventHandler};
use serenity::async_trait;
use serenity::model::guild::Guild;
use serenity::model::id::{ChannelId, GuildId};
use serenity::model::voice::VoiceState;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::session::RecordingSession;

/// Serenityのイベントを処理するハンドラ
pub struct BotHandler {
    /// 録音対象のボイスチャンネルID
    pub target_channel_id: ChannelId,
    /// 録音ファイルの出力ディレクトリ
    pub output_dir: PathBuf,
    /// 共有セッションへの参照
    pub session: Arc<Mutex<Option<RecordingSession>>>,
    /// カスタムステータスメッセージ
    pub custom_status: Option<String>,
}
impl BotHandler {
    /// ボイスチャンネルの状態を確認し、録音を開始または終了します。
    async fn check_and_manage_recording(&self, ctx: &Context, guild_id: GuildId) {
        let current_users = ctx.cache.guild(guild_id).map(|guild| {
            guild.voice_states.values()
                .filter(|vs| vs.channel_id == Some(self.target_channel_id) && vs.user_id != ctx.cache.current_user().id)
                .count()
        }).unwrap_or(0);

        if current_users > 0 {
            RecordingSession::start(
                guild_id,
                self.target_channel_id,
                ctx,
                &self.session,
                &self.output_dir,
            ).await;
        } else {
            let mut guard = self.session.lock().await;
            if let Some(session) = guard.take() {
                drop(guard);
                session.end().await;
            }
        }
    }
}
#[async_trait]
impl EventHandler for BotHandler {
    /// Botが起動したときに呼ばれます。
    async fn ready(&self, ctx: Context, ready: serenity::model::gateway::Ready) {
        println!("Botが起動しました: {}", ready.user.name);

        if let Some(status_text) = &self.custom_status {
            use serenity::gateway::ActivityData;
            let activity = ActivityData::custom(status_text);
            ctx.set_presence(Some(activity), serenity::model::user::OnlineStatus::Online);
            println!("カスタムステータスを設定しました: {}", status_text);
        }
    }

    /// ギルドデータが利用可能になったときに呼ばれます。
    async fn guild_create(&self, ctx: Context, guild: Guild, _is_new: Option<bool>) {
        self.check_and_manage_recording(&ctx, guild.id).await;
    }

    /// ボイス状態が変化したときに呼ばれます。
    async fn voice_state_update(&self, ctx: Context, old: Option<VoiceState>, new: VoiceState) {
        let Some(guild_id) = new.guild_id.or_else(|| old.and_then(|o| o.guild_id)) else { return; };
        self.check_and_manage_recording(&ctx, guild_id).await;
    }
}
