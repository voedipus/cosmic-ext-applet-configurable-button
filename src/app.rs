// SPDX-License-Identifier: MIT

use std::process::Command;
use std::sync::Arc;

use crate::config::Config;
use crate::fl;
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::{Limits, Subscription, window::Id};
use cosmic::iced_winit::commands::popup::{destroy_popup, get_popup};
use cosmic::widget::mouse_area;
use cosmic::{prelude::*, widget};
use futures_util::SinkExt;
use tokio::task::spawn_blocking;
use tracing::{error, info};

/// The application model stores app-specific state used to describe its interface and
/// drive its logic.
#[derive(Default)]
pub struct AppModel {
    /// Application state which is managed by the COSMIC runtime.
    core: cosmic::Core,
    /// The popup id.
    popup: Option<Id>,
    /// Configuration data that persists between application runs.
    config: Config,
    /// Example row toggler.
    status: bool,

    status_check_interval: Arc<u64>,
}

/// Messages emitted by the application and its widgets.
#[derive(Debug, Clone)]
pub enum Message {
    Toggle,
    ToggleSettings,
    PopupClosed(Id),
    CheckStatus,
    ScriptFinished,
    ScriptFailed,
    StatusChecked,
    StatusCheckFailed,
    UpdateConfig(Config),
    UpdOnScript(String),
    UpdOffScript(String),
    UpdStatusScript(String),
    UpdStatusCheckingInterval(String),
}

/// Create a COSMIC application from the app model
impl cosmic::Application for AppModel {
    /// The async executor that will be used to run your application's commands.
    type Executor = cosmic::executor::Default;

    /// Data that your application receives to its init method.
    type Flags = ();

    /// Messages which the application and its widgets will emit.
    type Message = Message;

    /// Unique identifier in RDNN (reverse domain name notation) format.
    const APP_ID: &'static str = "dev.voedipus.ConfigurableButton";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    /// Initializes the application with any given flags and startup commands.
    fn init(
        core: cosmic::Core,
        _flags: Self::Flags,
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {
        // Construct the app model with the runtime's core.
        let mut app = AppModel {
            core,
            config: cosmic_config::Config::new(Self::APP_ID, Config::VERSION)
                .map(|context| match Config::get_entry(&context) {
                    Ok(config) => config,
                    Err((_errors, config)) => {
                        // for why in errors {
                        //     tracing::error!(%why, "error loading app config");
                        // }

                        config
                    }
                })
                .unwrap_or_default(),
            ..Default::default()
        };
        app.status_check_interval = Arc::new(app.config.status_check_interval);

        (app, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    /// Describes the interface based on the current state of the application model.
    ///
    /// The applet's button in the panel will be drawn using the main view method.
    /// This view should emit messages to toggle the applet's popup window, which will
    /// be drawn using the `view_window` method.
    fn view(&self) -> Element<'_, Self::Message> {
        mouse_area(
            self.core
                .applet
                // .icon_button(if self.status {
                //     "emblem-default-symbolic"
                // } else {
                //     "emblem-important-symbolic"
                // })
                .icon_button_from_handle(if self.status {
                    widget::icon::from_name("emblem-default")
                        .symbolic(false)
                        .into()
                } else {
                    widget::icon::from_name("emblem-important")
                        .symbolic(false)
                        .into()
                })
                .on_press(Message::Toggle),
        )
        .on_right_release(Message::ToggleSettings)
        .into()
    }

    /// The applet's popup window will be drawn using this view method. If there are
    /// multiple poups, you may match the id parameter to determine which popup to
    /// create a view for.
    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let on_script_row = cosmic::iced_widget::column![
            cosmic::widget::text(fl!("on-script")),
            cosmic::widget::text_input(" ", &self.config.on_script)
                .on_input(|script| Message::UpdOnScript(script))
                .width(cosmic::iced::Length::Fill)
        ];
        let off_script_row = cosmic::iced_widget::column![
            cosmic::widget::text(fl!("off-script")),
            cosmic::widget::text_input(" ", &self.config.off_script)
                .on_input(|script| Message::UpdOffScript(script))
                .width(cosmic::iced::Length::Fill)
        ];
        let status_script_row = cosmic::iced_widget::column![
            cosmic::widget::text(fl!("status-script")),
            cosmic::widget::text_input(" ", &self.config.status_script)
                .on_input(|status| Message::UpdStatusScript(status))
                .width(cosmic::iced::Length::Fill)
        ];
        let intv = self.status_check_interval.to_string();
        let run_interval_row = cosmic::iced_widget::column![
            cosmic::widget::text(fl!("run-interval")),
            cosmic::widget::text_input(" ", intv)
                .on_input(|interval| Message::UpdStatusCheckingInterval(interval))
                .width(cosmic::iced::Length::Fill)
        ];
        let data = cosmic::iced_widget::column![
            cosmic::applet::padded_control(on_script_row),
            cosmic::applet::padded_control(off_script_row),
            cosmic::applet::padded_control(cosmic::widget::divider::horizontal::default()),
            cosmic::applet::padded_control(status_script_row),
            cosmic::applet::padded_control(run_interval_row),
        ]
        .padding([16, 0]);

        self.core
            .applet
            .popup_container(cosmic::widget::container(data))
            .into()
    }

    /// Register subscriptions for this application.
    ///
    /// Subscriptions are long-lived async tasks running in the background which
    /// emit messages to the application through a channel. They may be conditionally
    /// activated by selectively appending to the subscription batch, and will
    /// continue to execute for the duration that they remain in the batch.
    fn subscription(&self) -> Subscription<Self::Message> {
        struct MySubscription;

        let status_check_interval = self.status_check_interval.clone();

        Subscription::batch(vec![
            // Create a subscription which emits updates through a channel.
            Subscription::run_with_id(
                std::any::TypeId::of::<MySubscription>(),
                cosmic::iced::stream::channel(4, move |mut channel| async move {
                    loop {
                        _ = channel.send(Message::CheckStatus).await;
                        tokio::time::sleep(tokio::time::Duration::from_secs(
                            *status_check_interval,
                        ))
                        .await;
                    }
                }),
            ),
            // Watch for application configuration changes.
            self.core()
                .watch_config::<Config>(Self::APP_ID)
                .map(|update| Message::UpdateConfig(update.config)),
        ])
    }
    /// Handles messages emitted by the application and its widgets.
    ///
    /// Tasks may be returned for asynchronous execution of code in the background
    /// on the application's async runtime. The application will not exit until all
    /// tasks are finished.
    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::CheckStatus => {
                info!("Checking status");
                let script = self.config.status_script.clone();
                return cosmic::task::future(async move {
                    let result =
                        spawn_blocking(move || Command::new("sh").arg("-c").arg(script).status())
                            .await
                            .unwrap();

                    if let Ok(status) = result {
                        if status.success() {
                            Message::StatusChecked
                        } else {
                            Message::StatusCheckFailed
                        }
                    } else {
                        Message::StatusCheckFailed
                    }
                });
            }
            Message::UpdateConfig(config) => {
                self.config = config;
            }
            Message::Toggle => {
                let script = if self.status {
                    self.config.off_script.clone()
                } else {
                    self.config.on_script.clone()
                };
                info!("Executing script: {}", script);

                return cosmic::task::future(async move {
                    let result =
                        spawn_blocking(move || Command::new("sh").arg("-c").arg(script).status())
                            .await
                            .unwrap();
                    if let Ok(status) = result {
                        if status.success() {
                            Message::ScriptFinished
                        } else {
                            Message::ScriptFailed
                        }
                    } else {
                        Message::ScriptFailed
                    }
                });
            }
            Message::ScriptFinished => {
                info!("Script finished");
                self.status = !self.status;
            }
            Message::ScriptFailed => {
                info!("Script failed");
            }
            Message::StatusChecked => {
                self.status = true;
            }
            Message::StatusCheckFailed => {
                self.status = false;
            }
            Message::ToggleSettings => {
                return if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(372.0)
                        .min_width(300.0)
                        .min_height(200.0)
                        .max_height(1080.0);
                    get_popup(popup_settings)
                };
            }
            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }
            Message::UpdOnScript(cmd) => {
                self.config.on_script = cmd;
            }
            Message::UpdOffScript(cmd) => {
                self.config.off_script = cmd;
            }
            Message::UpdStatusScript(cmd) => {
                self.config.status_script = cmd;
            }
            Message::UpdStatusCheckingInterval(interval) => {
                self.config.status_check_interval = interval.parse().unwrap_or(10);
            }
        }
        Task::none()
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }
}
