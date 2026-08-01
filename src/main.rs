mod utils;
mod bot;
mod session;
mod track;

use clap::Parser;
use dotenvy::dotenv;
use serenity::Client;
use serenity::all::GatewayIntents;
use serenity::model::id::ChannelId;
use songbird::driver::{DecodeConfig, DecodeMode};
use songbird::SerenityInit;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::bot::Bot;
use crate::utils::signal::wait_for_shutdown;

/// コマンドライン引数
#[derive(Parser)]
#[command(name = "aoirecord", about = "Discord ボイスチャンネルの録音ツール")]
struct Args {
    /// Discord Bot トークン
    #[arg(env = "DISCORD_TOKEN")]
    token: String,

    /// 録音対象のボイスチャンネル ID
    #[arg(env = "DISCORD_CHANNEL_ID")]
    channel_id: u64,

    /// 録音ファイルの出力ディレクトリ
    #[arg(short, long, env = "OUTPUT_DIR", default_value = "output")]
    output: PathBuf,

    /// カスタムステータスメッセージ
    #[arg(short, long, env = "CUSTOM_STATUS")]
    status: Option<String>,
}

#[tokio::main]
async fn main() {
    dotenv().ok();

    let args = Args::parse();
    let intents = GatewayIntents::GUILDS
        | GatewayIntents::GUILD_VOICE_STATES
        | GatewayIntents::GUILD_MEMBERS;

    let handler = Bot {
        target_channel_id: ChannelId::new(args.channel_id),
        output_dir: args.output,
        session: Arc::new(Mutex::new(None)),
        custom_status: args.status,
    };
    let session = handler.session.clone();

    // 【最重要設定】Songbird内部で自動的に復号化とPCMデコードを行う
    let songbird_config = songbird::Config::default()
        .decode_mode(DecodeMode::Decode(DecodeConfig::new(
            songbird::driver::Channels::Stereo,
            songbird::driver::SampleRate::Hz48000,
        )));

    let mut client = Client::builder(&args.token, intents)
        .event_handler(handler)
        .register_songbird_from_config(songbird_config)
        .await
        .expect("クライアントの作成に失敗しました");

    let shard_manager = client.shard_manager.clone();
    tokio::spawn(async move {
        wait_for_shutdown().await;
        println!("\nシャットダウンシグナルを受信しました...");
        shard_manager.shutdown_all().await;

        // NOTE: 録音セッションをシャットダウンする
        let mut guard = session.lock().await;
        if let Some(session) = guard.take() {
            session.end().await;
            println!("録音セッションを終了しました。");
        }
    });

    if let Err(why) = client.start().await {
        eprintln!("クライアントの実行エラー: {:?}", why);
    }
}
