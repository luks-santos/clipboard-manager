use anyhow::Context;
use cosmic::iced::{Subscription, stream::channel};
use futures::{SinkExt, StreamExt, stream::BoxStream};
use std::{any::TypeId, future::pending};
use tokio::sync::mpsc;
use zbus::{interface, proxy};

use crate::message::AppMsg;

pub const REMOTE_SERVICE: &str =
    "io.github.cosmic_utils.cosmic-ext-applet-clipboard-manager.Remote";
pub const REMOTE_PATH: &str = "/io/github/cosmic_utils/cosmic_ext_applet_clipboard_manager/Remote";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteCommand {
    ToggleLauncher,
    ShowLauncher,
    Ping,
}

impl RemoteCommand {
    pub fn from_flag(flag: &str) -> Option<Self> {
        match flag {
            "--toggle-launcher" => Some(Self::ToggleLauncher),
            "--show-launcher" => Some(Self::ShowLauncher),
            "--ping" => Some(Self::Ping),
            _ => None,
        }
    }

    pub fn flag(self) -> &'static str {
        match self {
            Self::ToggleLauncher => "--toggle-launcher",
            Self::ShowLauncher => "--show-launcher",
            Self::Ping => "--ping",
        }
    }

    fn into_app_message(self) -> AppMsg {
        match self {
            Self::ToggleLauncher => AppMsg::ToggleLauncherRemote,
            Self::ShowLauncher => AppMsg::ShowLauncherRemote,
            Self::Ping => unreachable!("ping is handled directly by D-Bus"),
        }
    }
}

pub fn subscription() -> Subscription<AppMsg> {
    Subscription::run_with(TypeId::of::<RemoteSubscription>(), remote_stream)
}

fn remote_stream(_id: &TypeId) -> BoxStream<'static, AppMsg> {
    channel(20, async move |mut output| {
        let (tx, mut rx) = mpsc::unbounded_channel();

        let _connection = match start_service(tx).await {
            Ok(connection) => connection,
            Err(err) => {
                error!("failed to start remote D-Bus service: {err}");
                pending::<()>().await;
                unreachable!();
            }
        };

        while let Some(command) = rx.recv().await {
            if let Err(err) = output.send(command.into_app_message()).await {
                error!("failed to forward remote command to app state: {err}");
                break;
            }
        }

        pending::<()>().await;
    })
    .boxed()
}

pub fn invoke(command: RemoteCommand) -> anyhow::Result<()> {
    let connection =
        zbus::blocking::Connection::session().context("failed to connect to the session D-Bus")?;
    let proxy = ClipboardManagerRemoteProxyBlocking::new(&connection)
        .context("clipboard manager applet is not reachable over D-Bus")?;

    match command {
        RemoteCommand::ToggleLauncher => proxy
            .toggle_launcher()
            .context("failed to send toggle launcher request to the applet")?,
        RemoteCommand::ShowLauncher => proxy
            .show_launcher()
            .context("failed to send show launcher request to the applet")?,
        RemoteCommand::Ping => {
            anyhow::ensure!(
                proxy.ping().context("failed to ping the applet")?,
                "applet answered an unexpected ping response"
            );
            println!("pong");
        }
    }

    Ok(())
}

async fn start_service(tx: mpsc::UnboundedSender<RemoteCommand>) -> zbus::Result<zbus::Connection> {
    let connection = zbus::connection::Builder::session()?.build().await?;

    match connection
        .object_server()
        .at(REMOTE_PATH, RemoteService { tx })
        .await
    {
        Ok(true) => {}
        Ok(false) => {
            return Err(zbus::Error::Failure(format!(
                "remote interface is already registered at {REMOTE_PATH}"
            )));
        }
        Err(err) => return Err(err.into()),
    }

    connection.request_name(REMOTE_SERVICE).await?;

    Ok(connection)
}

struct RemoteService {
    tx: mpsc::UnboundedSender<RemoteCommand>,
}

struct RemoteSubscription;

impl RemoteService {
    fn send(&self, command: RemoteCommand) -> zbus::fdo::Result<()> {
        self.tx.send(command).map_err(|_| {
            zbus::fdo::Error::Failed("clipboard manager app state is no longer available".into())
        })
    }
}

#[proxy(
    interface = "io.github.cosmic_utils.cosmic_ext_applet_clipboard_manager.Remote",
    default_service = "io.github.cosmic_utils.cosmic-ext-applet-clipboard-manager.Remote",
    default_path = "/io/github/cosmic_utils/cosmic_ext_applet_clipboard_manager/Remote",
    gen_blocking = true,
    assume_defaults = true
)]
trait ClipboardManagerRemote {
    fn toggle_launcher(&self) -> zbus::Result<()>;
    fn show_launcher(&self) -> zbus::Result<()>;
    fn ping(&self) -> zbus::Result<bool>;
}

#[interface(name = "io.github.cosmic_utils.cosmic_ext_applet_clipboard_manager.Remote")]
impl RemoteService {
    fn toggle_launcher(&self) -> zbus::fdo::Result<()> {
        self.send(RemoteCommand::ToggleLauncher)
    }

    fn show_launcher(&self) -> zbus::fdo::Result<()> {
        self.send(RemoteCommand::ShowLauncher)
    }

    fn ping(&self) -> bool {
        true
    }
}
