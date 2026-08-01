/// SIGINT または SIGTERM を待機します。
pub async fn wait_for_shutdown() {
    #[cfg(unix)] {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigterm = signal(SignalKind::terminate()).expect("SIGTERMハンドラの設定に失敗");

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
    }

    #[cfg(not(unix))] {
        tokio::signal::ctrl_c().await.expect("Ctrl+Cシグナルの受信に失敗");
    }
 }
