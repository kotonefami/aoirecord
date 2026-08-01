# aoirecord

Rust で書かれた、Discord ボイスチャンネルの音声を Ogg Opus ファイルに録音する Bot。
依存クレートは serenity, songbird, audiopus など。

## ファイル構成
- `src/main.rs` - エントリーポイント
- `src/bot.rs` - Bot のロジック: `Bot`
- `src/session.rs` - 録音セッション（ボイスチャンネル）構造体: `Session`
- `src/track.rs` - 録音セッション中のユーザートラック（Opus ファイル）構造体: `Track`

## CI/CD
GitHub Actions での CI/CD を設定済み。
