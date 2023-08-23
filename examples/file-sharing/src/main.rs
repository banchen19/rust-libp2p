// 版权 2021 Protocol Labs。
//
// 特此免费授予任何获得本软件及其相关文档文件（以下称为“软件”）副本的人，
// 无偿使用软件，无论限制与否，包括但不限于使用、复制、修改、合并、出版、分发、
// 授予再许可以及/或出售软件的副本，以及允许接收软件的人这样做，
// 前提是遵守以下条件：
//
// 上述版权声明和本许可声明应包含在所有副本或实质部分中。
//
// 本软件按“原样”提供，无任何形式的明示或暗示保证，
// 包括但不限于对适销性、适用性的暗示保证和不侵权的保证。
// 在任何情况下，作者或版权持有人均不对任何索赔、损害或其他责任负责，
// 无论是在合同诉讼、侵权行为还是其他情况下产生的，
// 与软件或本软件的使用或其他处理有关，或者与软件或本软件的使用或其他处理有关。

#![doc = include_str!("../README.md")]

mod network;

use async_std::task::spawn;
use clap::Parser;

use futures::prelude::*;
use futures::StreamExt;
use libp2p::{core::Multiaddr, multiaddr::Protocol};
use std::error::Error;
use std::io::Write;
use std::path::PathBuf;

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let opt = Opt::parse();

    let (mut network_client, mut network_events, network_event_loop) =
        network::new(opt.secret_key_seed).await?;

    // 后台运行网络任务。
    spawn(network_event_loop.run());

    // 如果提供了监听地址，请使用它，否则监听任何地址。
    match opt.listen_address {
        Some(addr) => network_client
            .start_listening(addr)
            .await
            .expect("监听不应失败。"),
        None => network_client
            .start_listening("/ip4/0.0.0.0/tcp/0".parse()?)
            .await
            .expect("监听不应失败。"),
    };

    // 如果用户在命令行提供了对等方地址，请连接它。
    if let Some(addr) = opt.peer {
        let peer_id = match addr.iter().last() {
            Some(Protocol::P2p(peer_id)) => peer_id,
            _ => return Err("预期对等多地址包含对等方 ID。".into()),
        };
        network_client
            .dial(peer_id, addr)
            .await
            .expect("拨号应成功");
    }

    match opt.argument {
        // 提供文件。
        CliArgument::Provide { path, name } => {
            // 在 DHT 上广告自己作为文件提供者。
            network_client.start_providing(name.clone()).await;

            loop {
                match network_events.next().await {
                    // 在传入请求时回复文件内容。
                    Some(network::Event::InboundRequest { request, channel }) => {
                        if request == name {
                            network_client
                                .respond_file(std::fs::read(&path)?, channel)
                                .await;
                        }
                    }
                    e => todo!("{:?}", e),
                }
            }
        }
        // 定位并获取文件。
        CliArgument::Get { name } => {
            // 定位提供文件的所有节点。
            let providers = network_client.get_providers(name.clone()).await;
            if providers.is_empty() {
                return Err(format!("无法找到文件 {name} 的提供者。").into());
            }

            // 从每个节点请求文件内容。
            let requests = providers.into_iter().map(|p| {
                let mut network_client = network_client.clone();
                let name = name.clone();
                async move { network_client.request_file(p, name).await }.boxed()
            });

            // 等待请求，一旦其中一个成功，忽略其余请求。
            let file_content = futures::future::select_ok(requests)
                .await
                .map_err(|_| "没有提供者返回文件。")?
                .0;

            std::io::stdout().write_all(&file_content)?;
        }
    }

    Ok(())
}

#[derive(Parser, Debug)]
#[clap(name = "libp2p 文件共享示例")]
struct Opt {
    /// 生成确定性对等 ID 的固定值。
    #[clap(long)]
    secret_key_seed: Option<u8>,

    #[clap(long)]
    peer: Option<Multiaddr>,

    #[clap(long)]
    listen_address: Option<Multiaddr>,

    #[clap(subcommand)]
    argument: CliArgument,
}

#[derive(Debug, Parser)]
enum CliArgument {
    Provide {
        #[clap(long)]
        path: PathBuf,
        #[clap(long)]
        name: String,
    },
    Get {
        #[clap(long)]
        name: String,
    },
}
