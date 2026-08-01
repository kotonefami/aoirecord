# aoirecord

Rust で書かれた、Discord ボイスチャンネルの音声を Ogg Opus ファイルに録音する Bot。
依存クレートは serenity, songbird, audiopus など。

ロジックはすべて src/main.rs に書かれているため、修正の際はこのファイルのみ参照すればよい。
GitHub Actions での CI/CD を設定済み。
