use std::sync::Arc;
use tokio::sync::mpsc as tokio_mpsc;

use crate::{
    styles,
    util::{broadcast::broadcast_psbt, sign::sign_psbt, sync::sync_wallet, SyncResult},
};
use iced::{
    alignment,
    font::Font,
    widget::{
        button, checkbox, column, container, row, scrollable, text, text_input, Column, Space,
    },
    window, Element, Length, Padding, Size, Subscription, Task, Theme,
};
use miniscript::bitcoin::{self, Txid};

const GOLOS_TEXT: Font = Font::with_name("Golos Text");

#[derive(Debug, Clone)]
enum Message {
    DescriptorChanged(String),
    AutoFormatDescriptorToggled(bool),
    IpChanged(String),
    PortChanged(String),
    TargetChanged(String),
    AddressChanged(String),
    MaxChanged(String),
    BatchChanged(String),
    FeeChanged(String),
    MnemonicChanged(String),
    #[allow(unused)]
    PsbtInputChanged(String),
    SyncClicked,
    SignClicked,
    BroadcastClicked,
    SyncCompleted(Result<SyncResult, String>),
    SignCompleted(Result<String, String>),
    BroadcastCompleted(Result<Txid, String>),
    ClearLogs,
    ToggleConfig,
    ToggleMnemonic,
    ToggleMnemonicVisibility,
    LogUpdate(String),
    CopyPsbt,
}

struct WalletApp {
    network: bitcoin::Network,
    descriptor: String,
    ip: String,
    port: String,
    target: String,
    address: String,
    max: String,
    batch: String,
    fee: String,
    mnemonic: String,
    psbt_input: String,
    status: String,
    logs: Vec<String>,
    synced_psbt: Option<SyncResult>,
    signed_psbt: Option<String>,
    broadcast_txid: Option<Txid>,
    is_processing: bool,
    config_expanded: bool,
    mnemonic_expanded: bool,
    auto_format_descriptor: bool,
    mnemonic_visible: bool,
    log_receiver: Option<Arc<tokio::sync::Mutex<tokio_mpsc::UnboundedReceiver<String>>>>,
}

impl Default for WalletApp {
    fn default() -> Self {
        Self {
            network: bitcoin::Network::Bitcoin,
            descriptor: String::new(),
            ip: String::from("ssl://fulcrum.bullbitcoin.com"),
            port: String::from("50002"),
            target: String::from("10000"),
            address: String::new(),
            max: String::from("20000"),
            batch: String::from("10000"),
            fee: String::from("1"),
            mnemonic: String::new(),
            psbt_input: String::new(),
            status: String::from("Ready"),
            logs: Vec::new(),
            synced_psbt: None,
            signed_psbt: None,
            broadcast_txid: None,
            is_processing: false,
            config_expanded: true,
            mnemonic_expanded: false,
            mnemonic_visible: false,
            auto_format_descriptor: true,
            log_receiver: None,
        }
    }
}

impl WalletApp {
    pub fn new(network: bitcoin::Network) -> (Self, Task<Message>) {
        let app = Self {
            network,
            ..Default::default()
        };
        (app, Task::none())
    }

    pub fn title(&self) -> String {
        let network = match self.network {
            bitcoin::Network::Bitcoin => String::new(),
            n => format!("({n})"),
        };
        format!("SPK Recovery Tool {network}")
    }

    pub fn theme(&self) -> Theme {
        Theme::Light
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::DescriptorChanged(value) => {
                self.descriptor = value;
                Task::none()
            }
            Message::AutoFormatDescriptorToggled(value) => {
                self.auto_format_descriptor = value;
                Task::none()
            }
            Message::IpChanged(value) => {
                self.ip = value;
                Task::none()
            }
            Message::PortChanged(value) => {
                self.port = value;
                Task::none()
            }
            Message::TargetChanged(value) => {
                self.target = value;
                Task::none()
            }
            Message::AddressChanged(value) => {
                self.address = value;
                Task::none()
            }
            Message::MaxChanged(value) => {
                self.max = value;
                Task::none()
            }
            Message::BatchChanged(value) => {
                self.batch = value;
                Task::none()
            }
            Message::FeeChanged(value) => {
                self.fee = value;
                Task::none()
            }
            Message::MnemonicChanged(value) => {
                self.mnemonic = value;
                Task::none()
            }
            Message::PsbtInputChanged(value) => {
                self.psbt_input = value;
                Task::none()
            }
            Message::SyncClicked => {
                if self.is_processing {
                    return Task::none();
                }

                self.is_processing = true;
                self.config_expanded = false;
                self.status = String::from("Syncing...");
                self.logs.clear();

                if self.auto_format_descriptor {
                    self.descriptor = format_descriptor(&self.descriptor);
                }

                let (log_tx, log_rx) = tokio_mpsc::unbounded_channel();
                self.log_receiver = Some(Arc::new(tokio::sync::Mutex::new(log_rx)));

                let _ = log_tx.send("=== Configuration ===".to_string());
                let _ = log_tx.send(format!("Electrum: {}:{}", self.ip, self.port));
                let _ = log_tx.send(format!("Target index: {}", self.target));
                let _ = log_tx.send(format!("Max subscriptions: {}", self.max));
                let _ = log_tx.send(format!("Batch size: {}", self.batch));
                let _ = log_tx.send(format!("Fee rate: {} sat/vB", self.fee));
                let _ = log_tx.send("===================".to_string());

                let descriptor = self.descriptor.clone();
                let ip = self.ip.clone();
                let port = self.port.clone();
                let target = self.target.clone();
                let address = self.address.clone();
                let max = self.max.clone();
                let batch = self.batch.clone();
                let fee = self.fee.clone();
                let network = self.network;

                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            sync_wallet(
                                descriptor, ip, port, target, address, max, batch, fee, log_tx,
                                network,
                            )
                        })
                        .await
                        .map_err(|e| format!("Task error: {}", e))?
                    },
                    Message::SyncCompleted,
                )
            }
            Message::SignClicked => {
                if self.is_processing {
                    return Task::none();
                }

                if self.mnemonic.trim().is_empty() {
                    self.status = String::from("Error: Mnemonic is required for signing");
                    return Task::none();
                }

                let psbt_str = if self.psbt_input.is_empty() {
                    if let Some(ref sync_result) = self.synced_psbt {
                        sync_result.psbt.clone()
                    } else {
                        self.status = String::from("Error: No PSBT to sign");
                        return Task::none();
                    }
                } else {
                    self.psbt_input.clone()
                };

                self.is_processing = true;
                self.status = String::from("Signing...");

                let mnemonic = self.mnemonic.clone();
                let descriptor = self.descriptor.clone();
                let network = self.network;

                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            sign_psbt(mnemonic, psbt_str, descriptor, network)
                        })
                        .await
                        .map_err(|e| format!("Task error: {}", e))?
                    },
                    Message::SignCompleted,
                )
            }
            Message::BroadcastClicked => {
                if self.is_processing {
                    return Task::none();
                }

                let psbt_str = if let Some(ref signed) = self.signed_psbt {
                    signed.clone()
                } else if !self.psbt_input.is_empty() {
                    self.psbt_input.clone()
                } else {
                    self.status = String::from("Error: No signed PSBT to broadcast");
                    return Task::none();
                };

                self.is_processing = true;
                self.status = String::from("Broadcasting...");

                let ip = self.ip.clone();
                let port = self.port.clone();

                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || broadcast_psbt(psbt_str, ip, port))
                            .await
                            .map_err(|e| format!("Task error: {}", e))?
                    },
                    Message::BroadcastCompleted,
                )
            }
            Message::SyncCompleted(result) => {
                self.is_processing = false;
                self.log_receiver = None;
                match result {
                    Ok(sync_result) => {
                        self.status = format!(
                            "Sync completed: {} inputs, {} BTC total",
                            sync_result.num_inputs,
                            sync_result.total_value.to_btc()
                        );
                        self.psbt_input = sync_result.psbt.clone();
                        self.synced_psbt = Some(sync_result);
                        self.mnemonic_expanded = true;
                    }
                    Err(e) => {
                        self.status = format!("Sync error: {}", e);
                        self.logs.push(format!("ERROR: {}", e));
                    }
                }
                Task::none()
            }
            Message::SignCompleted(result) => {
                self.is_processing = false;
                match result {
                    Ok(signed_psbt) => {
                        self.status = String::from("PSBT signed successfully");
                        self.signed_psbt = Some(signed_psbt);
                    }
                    Err(e) => {
                        self.status = format!("Sign error: {}", e);
                    }
                }
                Task::none()
            }
            Message::BroadcastCompleted(result) => {
                self.is_processing = false;
                match result {
                    Ok(txid) => {
                        self.status = format!("Broadcast successful: {}", txid);
                        self.logs.push(format!("Transaction broadcast: {}", txid));
                        self.broadcast_txid = Some(txid);
                    }
                    Err(e) => {
                        self.status = format!("Broadcast error: {}", e);
                        self.logs.push(format!("Broadcast ERROR: {}", e));
                    }
                }
                Task::none()
            }
            Message::ClearLogs => {
                self.logs.clear();
                Task::none()
            }
            Message::ToggleConfig => {
                self.config_expanded = !self.config_expanded;
                Task::none()
            }
            Message::ToggleMnemonic => {
                self.mnemonic_expanded = !self.mnemonic_expanded;
                Task::none()
            }
            Message::ToggleMnemonicVisibility => {
                self.mnemonic_visible = !self.mnemonic_visible;
                Task::none()
            }
            Message::LogUpdate(msg) => {
                if msg.starts_with("STATUS:") {
                    self.status = msg.strip_prefix("STATUS:").unwrap_or(&msg).to_string();
                } else {
                    self.logs.insert(0, msg);
                }
                Task::none()
            }
            Message::CopyPsbt => {
                let psbt = self
                    .synced_psbt
                    .as_ref()
                    .map(|r| r.psbt.clone())
                    .unwrap_or_else(|| self.signed_psbt.clone().unwrap_or_default());
                if psbt.is_empty() {
                    return Task::none();
                }
                self.status = String::from("PSBT copied to clipboard");
                iced::clipboard::write(psbt)
            }
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        if let Some(ref receiver) = self.log_receiver {
            let receiver = Arc::clone(receiver);
            Subscription::run_with_id("log_stream", log_stream(receiver))
        } else {
            Subscription::none()
        }
    }

    pub fn view(&self) -> Element<Message> {
        let title = text("SPK Recovery Tool")
            .size(32)
            .font(GOLOS_TEXT)
            .width(Length::Fill)
            .align_x(alignment::Horizontal::Center)
            .color(styles::TEXT);

        let subtitle = text(&self.status)
            .size(14)
            .font(GOLOS_TEXT)
            .width(Length::Fill)
            .align_x(alignment::Horizontal::Center)
            .color(styles::TEXT_MUTED);

        let header = column![title, subtitle]
            .spacing(8)
            .padding(Padding::new(0.0).bottom(8.0));

        let config_section = self.build_config_section();
        let logs_section = self.build_logs_section();
        let sign_section = self.build_sign_section();

        let footer = text("by developers at Bull & Liana")
            .size(11)
            .font(GOLOS_TEXT)
            .width(Length::Fill)
            .align_x(alignment::Horizontal::Center)
            .color(styles::TEXT_MUTED);

        let content = column![
            header,
            config_section,
            logs_section,
            sign_section,
            Space::with_height(20),
            footer,
            Space::with_height(8),
        ]
        .spacing(20)
        .padding(Padding::new(24.0))
        .width(Length::Fill);

        let scrollable_content = scrollable(content).width(Length::Fill).height(Length::Fill);

        container(scrollable_content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| iced::widget::container::Style {
                background: Some(iced::Background::Color(styles::BACKGROUND)),
                ..Default::default()
            })
            .into()
    }

    fn build_config_section(&self) -> Element<Message> {
        let header = row![
            button(
                text(if self.config_expanded { "▼" } else { "▶" })
                    .font(GOLOS_TEXT)
                    .size(14)
            )
            .on_press(Message::ToggleConfig)
            .padding([4, 8])
            .style(|_theme, status| styles::secondary_button(status)),
            text("Sync Config")
                .size(20)
                .font(GOLOS_TEXT)
                .color(styles::TEXT),
        ]
        .spacing(10)
        .align_y(alignment::Vertical::Center);

        if !self.config_expanded {
            return column![header].spacing(12).width(Length::Fill).into();
        }

        let label_width = Length::Fixed(170.0);

        let fields = column![
            row![
                text("Descriptor:")
                    .width(label_width)
                    .font(GOLOS_TEXT)
                    .color(styles::GREY_DARK),
                text_input(
                    "wpkh([fingerprint/84h/0h/0h]xpub.../<0;1>/*)",
                    &self.descriptor
                )
                .on_input(Message::DescriptorChanged)
                .width(Length::Fill)
                .font(GOLOS_TEXT)
                .padding(10)
                .style(|_theme, status| styles::styled_text_input(status)),
            ]
            .spacing(12)
            .align_y(alignment::Vertical::Center),
            row![
                Space::with_width(label_width),
                checkbox(
                    "Auto-format descriptor to include change",
                    self.auto_format_descriptor
                )
                .on_toggle(Message::AutoFormatDescriptorToggled)
                .font(GOLOS_TEXT)
                .size(16)
                .text_size(14),
            ]
            .spacing(12)
            .align_y(alignment::Vertical::Center),
            row![
                text("Electrum IP:")
                    .width(label_width)
                    .font(GOLOS_TEXT)
                    .color(styles::GREY_DARK),
                text_input("ssl://fulcrum.bullbitcoin.com", &self.ip)
                    .on_input(Message::IpChanged)
                    .width(Length::Fill)
                    .font(GOLOS_TEXT)
                    .padding(10)
                    .style(|_theme, status| styles::styled_text_input(status)),
            ]
            .spacing(12)
            .align_y(alignment::Vertical::Center),
            row![
                text("Electrum Port:")
                    .width(label_width)
                    .font(GOLOS_TEXT)
                    .color(styles::GREY_DARK),
                text_input("50002", &self.port)
                    .on_input(Message::PortChanged)
                    .width(Length::Fill)
                    .font(GOLOS_TEXT)
                    .padding(10)
                    .style(|_theme, status| styles::styled_text_input(status)),
            ]
            .spacing(12)
            .align_y(alignment::Vertical::Center),
            row![
                text("Target Index:")
                    .width(label_width)
                    .font(GOLOS_TEXT)
                    .color(styles::GREY_DARK),
                text_input("10000", &self.target)
                    .on_input(Message::TargetChanged)
                    .width(Length::Fill)
                    .font(GOLOS_TEXT)
                    .padding(10)
                    .style(|_theme, status| styles::styled_text_input(status)),
            ]
            .spacing(12)
            .align_y(alignment::Vertical::Center),
            row![
                text("Destination Address:")
                    .width(label_width)
                    .font(GOLOS_TEXT)
                    .color(styles::GREY_DARK),
                text_input("Bitcoin address", &self.address)
                    .on_input(Message::AddressChanged)
                    .width(Length::Fill)
                    .font(GOLOS_TEXT)
                    .padding(10)
                    .style(|_theme, status| styles::styled_text_input(status)),
            ]
            .spacing(12)
            .align_y(alignment::Vertical::Center),
            row![
                text("Max Subscriptions:")
                    .width(label_width)
                    .font(GOLOS_TEXT)
                    .color(styles::GREY_DARK),
                text_input("20000", &self.max)
                    .on_input(Message::MaxChanged)
                    .width(Length::Fill)
                    .font(GOLOS_TEXT)
                    .padding(10)
                    .style(|_theme, status| styles::styled_text_input(status)),
            ]
            .spacing(12)
            .align_y(alignment::Vertical::Center),
            row![
                text("Batch Size:")
                    .width(label_width)
                    .font(GOLOS_TEXT)
                    .color(styles::GREY_DARK),
                text_input("10000", &self.batch)
                    .on_input(Message::BatchChanged)
                    .width(Length::Fill)
                    .font(GOLOS_TEXT)
                    .padding(10)
                    .style(|_theme, status| styles::styled_text_input(status)),
            ]
            .spacing(12)
            .align_y(alignment::Vertical::Center),
            row![
                text("Fee Rate (sat/vB):")
                    .width(label_width)
                    .font(GOLOS_TEXT)
                    .color(styles::GREY_DARK),
                text_input("1", &self.fee)
                    .on_input(Message::FeeChanged)
                    .width(Length::Fill)
                    .font(GOLOS_TEXT)
                    .padding(10)
                    .style(|_theme, status| styles::styled_text_input(status)),
            ]
            .spacing(12)
            .align_y(alignment::Vertical::Center),
        ]
        .spacing(10);

        let sync_button = button(
            text("Sync & Create PSBT")
                .font(GOLOS_TEXT)
                .size(16)
                .width(Length::Fill)
                .align_x(alignment::Horizontal::Center),
        )
        .padding([12, 24])
        .width(Length::Fixed(220.0))
        .style(|_theme, status| styles::primary_button(status))
        .on_press_maybe(if !self.is_processing {
            Some(Message::SyncClicked)
        } else {
            None
        });

        let body = column![
            Space::with_height(4),
            fields,
            Space::with_height(8),
            container(sync_button)
                .width(Length::Fill)
                .align_x(alignment::Horizontal::Center),
        ]
        .spacing(0);

        container(column![header, Space::with_height(16), body].spacing(0))
            .padding(20)
            .width(Length::Fill)
            .style(|_theme| styles::card_container())
            .into()
    }

    fn build_sign_section(&self) -> Element<Message> {
        let has_psbt = self.synced_psbt.is_some() || !self.psbt_input.is_empty();
        if !has_psbt && self.signed_psbt.is_none() && self.broadcast_txid.is_none() {
            return Space::with_height(0).into();
        }

        let header = row![
            button(
                text(if self.mnemonic_expanded { "▼" } else { "▶" })
                    .font(GOLOS_TEXT)
                    .size(14)
            )
            .on_press(Message::ToggleMnemonic)
            .padding([4, 8])
            .style(|_theme, status| styles::secondary_button(status)),
            text("Sign & Broadcast")
                .size(20)
                .font(GOLOS_TEXT)
                .color(styles::TEXT),
        ]
        .spacing(10)
        .align_y(alignment::Vertical::Center);

        if !self.mnemonic_expanded {
            return container(column![header])
                .padding(20)
                .width(Length::Fill)
                .style(|_theme| styles::card_container())
                .into();
        }

        let mut body = Column::new().spacing(12);

        // PSBT summary decoded from the PSBT string
        let psbt_str_for_decode: Option<&str> = if let Some(ref sr) = self.synced_psbt {
            Some(&sr.psbt)
        } else if !self.psbt_input.is_empty() {
            Some(&self.psbt_input)
        } else {
            None
        };

        if let Some(psbt_str) = psbt_str_for_decode {
            let mut summary_col = Column::new().spacing(6);

            if let Some((outputs, fees, inputs_count)) = decode_psbt_outputs(psbt_str) {
                summary_col = summary_col.push(
                    row![
                        text("Inputs:")
                            .width(Length::Fixed(60.0))
                            .font(GOLOS_TEXT)
                            .size(13)
                            .color(styles::GREY_DARK),
                        text(format!("{}", inputs_count))
                            .font(GOLOS_TEXT)
                            .size(13)
                            .color(styles::TEXT),
                    ]
                    .spacing(12),
                );
                summary_col = summary_col.push(Space::with_height(4));
                summary_col = summary_col.push(
                    text("Outputs")
                        .size(14)
                        .font(GOLOS_TEXT)
                        .color(styles::GREY_DARK),
                );
                for (addr, sats) in &outputs {
                    let btc = *sats as f64 / 100_000_000.0;
                    summary_col = summary_col.push(
                        row![
                            text(addr.clone())
                                .font(Font::MONOSPACE)
                                .size(13)
                                .color(styles::TEXT),
                            text(format!("{:.8} BTC", btc))
                                .font(GOLOS_TEXT)
                                .size(13)
                                .color(styles::TEXT_MUTED),
                        ]
                        .spacing(12),
                    );
                }
                if let Some(fee_sats) = fees {
                    summary_col = summary_col.push(Space::with_height(4));
                    summary_col = summary_col.push(
                        row![
                            text("Fees:")
                                .width(Length::Fixed(60.0))
                                .font(GOLOS_TEXT)
                                .size(13)
                                .color(styles::GREY_DARK),
                            text(format!("{} sats", fee_sats))
                                .font(GOLOS_TEXT)
                                .size(13)
                                .color(styles::TEXT),
                        ]
                        .spacing(12),
                    );
                }
            } else {
                summary_col = summary_col.push(
                    text("PSBT loaded (unable to decode details)")
                        .size(14)
                        .font(GOLOS_TEXT)
                        .color(styles::GREY_DARK),
                );
            }

            let copy_row = row![
                summary_col,
                Space::with_width(Length::Fill),
                button(
                    text("Copy PSBT")
                        .font(GOLOS_TEXT)
                        .size(14)
                        .align_x(alignment::Horizontal::Center),
                )
                .padding([10, 16])
                .style(|_theme, status| styles::secondary_button(status))
                .on_press(Message::CopyPsbt),
            ]
            .spacing(12)
            .align_y(alignment::Vertical::Top);

            body = body.push(copy_row);
            body = body.push(Space::with_height(4));
        }

        // Mnemonic input
        let mnemonic_row = row![
            text_input("Enter your 12 or 24 word mnemonic", &self.mnemonic)
                .on_input(Message::MnemonicChanged)
                .secure(!self.mnemonic_visible)
                .width(Length::Fill)
                .font(GOLOS_TEXT)
                .padding(10)
                .style(|_theme, status| styles::styled_text_input(status)),
            button(
                text(if self.mnemonic_visible {
                    "HIDE"
                } else {
                    "SHOW"
                })
                .font(GOLOS_TEXT)
                .size(12)
            )
            .on_press(Message::ToggleMnemonicVisibility)
            .padding([10, 14])
            .style(|_theme, status| styles::secondary_button(status)),
        ]
        .spacing(10)
        .align_y(alignment::Vertical::Center);

        let has_mnemonic = !self.mnemonic.trim().is_empty();

        let action_buttons = row![
            button(
                text("Sign PSBT")
                    .font(GOLOS_TEXT)
                    .size(16)
                    .width(Length::Fill)
                    .align_x(alignment::Horizontal::Center),
            )
            .padding([12, 24])
            .width(Length::Fixed(160.0))
            .style(|_theme, status| styles::primary_button(status))
            .on_press_maybe(if !self.is_processing && has_mnemonic {
                Some(Message::SignClicked)
            } else {
                None
            }),
            button(
                text("Broadcast")
                    .font(GOLOS_TEXT)
                    .size(16)
                    .width(Length::Fill)
                    .align_x(alignment::Horizontal::Center),
            )
            .padding([12, 24])
            .width(Length::Fixed(160.0))
            .style(|_theme, status| styles::primary_button(status))
            .on_press_maybe(if !self.is_processing && has_mnemonic {
                Some(Message::BroadcastClicked)
            } else {
                None
            }),
        ]
        .spacing(16);

        body = body.push(mnemonic_row);
        body = body.push(action_buttons);

        // Signed PSBT result
        if let Some(ref signed) = self.signed_psbt {
            body = body.push(Space::with_height(8));
            body = body.push(
                text("Signed PSBT")
                    .size(14)
                    .font(GOLOS_TEXT)
                    .color(styles::GREY_DARK),
            );
            body = body.push(
                text_input("", signed)
                    .width(Length::Fill)
                    .font(GOLOS_TEXT)
                    .padding(10)
                    .style(|_theme, status| styles::styled_text_input(status)),
            );
        }

        // Broadcast txid result
        if let Some(ref txid) = self.broadcast_txid {
            body = body.push(Space::with_height(8));
            body = body.push(
                text("Transaction ID")
                    .size(14)
                    .font(GOLOS_TEXT)
                    .color(styles::GREY_DARK),
            );
            body = body.push(
                text(format!("{}", txid))
                    .font(Font::MONOSPACE)
                    .size(13)
                    .color(styles::TEXT),
            );
        }

        container(column![header, Space::with_height(16), body].spacing(0))
            .padding(20)
            .width(Length::Fill)
            .style(|_theme| styles::card_container())
            .into()
    }

    fn build_logs_section(&self) -> Element<Message> {
        let logs_header = row![
            text("Logs").size(18).font(GOLOS_TEXT).color(styles::TEXT),
            Space::with_width(Length::Fill),
            button(
                text("Clear")
                    .size(14)
                    .font(GOLOS_TEXT)
                    .align_x(alignment::Horizontal::Center),
            )
            .padding([6, 12])
            .style(|_theme, status| styles::secondary_button(status))
            .on_press(Message::ClearLogs),
        ]
        .align_y(alignment::Vertical::Center);

        let log_content: Element<_> = if self.logs.is_empty() {
            container(
                text("No logs yet...")
                    .size(14)
                    .font(GOLOS_TEXT)
                    .color(styles::TEXT_MUTED),
            )
            .padding(16)
            .width(Length::Fill)
            .height(Length::Fixed(300.0))
            .style(|_theme| styles::log_container())
            .into()
        } else {
            let log_items: Vec<Element<Message>> = self
                .logs
                .iter()
                .map(|log| {
                    text(log)
                        .size(13)
                        .font(Font::MONOSPACE)
                        .color(styles::TEXT)
                        .into()
                })
                .collect();

            container(
                scrollable(
                    container(Column::with_children(log_items).spacing(4))
                        .padding(16)
                        .width(Length::Fill),
                )
                .height(Length::Fixed(300.0)),
            )
            .width(Length::Fill)
            .height(Length::Fixed(300.0))
            .style(|_theme| styles::log_container())
            .into()
        };

        column![logs_header, log_content]
            .spacing(12)
            .width(Length::Fill)
            .into()
    }
}

#[allow(clippy::type_complexity)]
fn decode_psbt_outputs(psbt_str: &str) -> Option<(Vec<(String, u64)>, Option<u64>, usize)> {
    use miniscript::bitcoin::{Address, Network, Psbt};
    use std::str::FromStr;

    let psbt = Psbt::from_str(psbt_str.trim()).ok()?;

    let inputs_count = psbt.inputs.len();
    let outputs: Vec<(String, u64)> = psbt
        .unsigned_tx
        .output
        .iter()
        .map(|o| {
            let addr = Address::from_script(&o.script_pubkey, Network::Bitcoin)
                .map(|a| a.to_string())
                .unwrap_or_else(|_| "Non-standard script".to_string());
            (addr, o.value.to_sat())
        })
        .collect();

    let total_in: Option<u64> = psbt
        .inputs
        .iter()
        .map(|i| i.witness_utxo.as_ref().map(|u| u.value.to_sat()))
        .try_fold(0u64, |acc, v| v.map(|a| acc + a));
    let total_out: u64 = psbt
        .unsigned_tx
        .output
        .iter()
        .map(|o| o.value.to_sat())
        .sum();
    let fees = total_in.map(|ti| ti.saturating_sub(total_out));

    Some((outputs, fees, inputs_count))
}

fn format_descriptor(s: &str) -> String {
    // Strip checksum (#...)
    let s = if let Some(idx) = s.find('#') {
        &s[..idx]
    } else {
        s
    };
    // Replace /0/* with /<0;1>/*
    s.replace("/0/*", "/<0;1>/*")
}

fn log_stream(
    receiver: Arc<tokio::sync::Mutex<tokio_mpsc::UnboundedReceiver<String>>>,
) -> impl futures::Stream<Item = Message> {
    futures::stream::unfold(receiver, |receiver| async move {
        let mut rx = receiver.lock().await;
        if let Some(log) = rx.recv().await {
            drop(rx);
            Some((Message::LogUpdate(log), receiver.clone()))
        } else {
            None
        }
    })
}

pub fn run(network: bitcoin::Network) -> Result<(), iced::Error> {
    iced::application(WalletApp::title, WalletApp::update, WalletApp::view)
        .theme(WalletApp::theme)
        .subscription(WalletApp::subscription)
        .window(window::Settings {
            size: Size::new(860.0, 900.0),
            min_size: Some(Size::new(600.0, 700.0)),
            ..Default::default()
        })
        .font(include_bytes!("../assets/GolosText-Regular.ttf"))
        .run_with(move || WalletApp::new(network))
}
