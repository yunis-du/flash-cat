use std::{
    env,
    io::{Write, stdout},
    path::PathBuf,
    process,
    sync::Arc,
};

use anyhow::Result;
use tokio_stream::StreamExt;

use flash_cat_common::{Shutdown, proto::ClientType, utils::gen_share_code};
use flash_cat_core::{RelayType, SenderInteractionMessage, sender::FlashCatSender};

use crate::progress::Progress;

#[derive(Clone)]
pub struct Send {
    share_code: String,
    sender: FlashCatSender,
    relay: Option<String>,

    shutdown: Shutdown,
}

impl Send {
    pub async fn new(
        zip: bool,
        relay: Option<String>,
        files: Vec<String>,
    ) -> Result<Self> {
        let files = files
            .into_iter()
            .map(|f| {
                if f == "." || f == "./" {
                    return env::current_dir().unwrap_or(PathBuf::from(".")).to_str().unwrap_or(".").to_string();
                }
                f
            })
            .collect::<Vec<_>>();
        let share_code = gen_share_code();
        let sender = FlashCatSender::new(share_code.clone(), relay.clone(), files, zip, ClientType::Cli).await?;
        Ok(Self {
            share_code,
            sender,
            relay,
            shutdown: Shutdown::new(),
        })
    }

    pub async fn run(&self) -> Result<()> {
        let file_collector = self.sender.get_file_collector();
        if file_collector.num_files == 1 {
            print!("Sending {} file ", file_collector.num_files);
        } else {
            print!("Sending {} files ", file_collector.num_files);
        }

        if file_collector.num_folders > 0 {
            if file_collector.num_files == 1 {
                print!("and {} folder ", file_collector.num_folders);
            } else {
                print!("and {} folders ", file_collector.num_folders);
            }
        }
        println!("({})", file_collector.total_size_to_human_readable());
        println!("Share code is: {}", self.share_code);
        println!("On the other computer run:");
        println!();
        if let Some(relay) = &self.relay {
            println!("flash-cat recv {} --relay {}", self.share_code, relay);
        } else {
            println!("flash-cat recv {}", self.share_code);
        }

        let mut progress = Progress::new(file_collector.num_files, file_collector.max_file_name_length);

        for file in file_collector.files.iter() {
            progress.add_progress(&file.name, file.file_id, file.size);
        }

        let mut stream = Arc::new(self.sender.clone()).start().await?;
        while !self.shutdown.is_terminated() {
            if let Some(sender_msg) = stream.next().await {
                match sender_msg {
                    SenderInteractionMessage::Message(msg) => println!("{msg}"),
                    SenderInteractionMessage::Error(e) => {
                        println!("An error occurred: {}", e.to_string());
                        self.shutdown();
                    }
                    SenderInteractionMessage::ReceiverReject => {
                        println!("Receiver reject this share. exit...");
                        self.shutdown();
                    }
                    SenderInteractionMessage::RelayFailed((relay_type, error)) => {
                        if RelayType::Local.eq(&relay_type) || RelayType::Specify.eq(&relay_type) {
                            process::exit(1);
                        } else {
                            println!("connect to {} relay failed: {}", relay_type.to_string(), error);
                        }
                    }
                    SenderInteractionMessage::ContinueFile(file_id) => {
                        progress.skip(file_id);
                    }
                    SenderInteractionMessage::FileProgress(file_progress) => {
                        progress.set_position(file_progress.file_id, file_progress.position);
                    }
                    SenderInteractionMessage::FileProgressFinish(file_id) => {
                        progress.finish(file_id);
                    }
                    SenderInteractionMessage::OtherClose => {
                        println!("The receive end is interrupted. exit...");
                        self.shutdown();
                    }
                    SenderInteractionMessage::SendDone => {
                        print!("Send files done. don't close this window, waiting for the receiver to receive finish...");
                        stdout().flush()?;
                    }
                    SenderInteractionMessage::Completed => {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        self.shutdown();
                    }
                }
            }
        }

        print!("\r{}", format!("{:<width$}", "", width = 100));
        stdout().flush()?;
        print!("\r");
        stdout().flush()?;
        Ok(())
    }

    pub fn shutdown(&self) {
        self.sender.shutdown();
        self.shutdown.shutdown();
    }

    pub async fn terminated(&self) {
        self.shutdown.wait().await
    }
}
