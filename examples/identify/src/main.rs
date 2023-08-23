// 版权 2018 Parity Technologies (英国) 有限公司
//
// 特此免费授予获得本软件及其关联文档文件（以下简称“软件”）副本的任何人
// 以无限制地处理本软件，包括但不限于使用、复制、修改、合并、发布、分发、再授权，
// 以及销售软件的副本，只需遵守以下条件：
//
// 上述版权声明和本许可声明应包含在
// 所有副本或主要部分中。
//
// 本“软件”按“原样”提供，不附带任何明示或暗示的保证，
// 包括但不限于适销性、特定用途适用性和非侵权性保证。在任何情况下，
// 作者或版权持有人均不对任何索赔、损害或其他责任承担责任，
// 无论是在合同诉讼、侵权行为还是其他情况下产生的，
// 从而导致软件或使用或其他交易中的其他交易产生的责任。

#![doc = include_str!("../README.md")]

use futures::prelude::*;
use libp2p::{
    core::{multiaddr::Multiaddr, upgrade::Version},
    identify, identity, noise,
    swarm::{SwarmBuilder, SwarmEvent},
    tcp, yamux, PeerId, Transport,
};
use std::error::Error;

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("本地对等 ID: {local_peer_id:?}");

    let transport = tcp::async_io::Transport::default()
        .upgrade(Version::V1Lazy)
        .authenticate(noise::Config::new(&local_key).unwrap())
        .multiplex(yamux::Config::default())
        .boxed();

    // 创建一个身份网络行为。
    let behaviour = identify::Behaviour::new(identify::Config::new(
        "/ipfs/id/1.0.0".to_string(),
        local_key.public(),
    ));

    let mut swarm =
        SwarmBuilder::with_async_std_executor(transport, behaviour, local_peer_id).build();

    // 告诉 Swarm 在所有接口上监听和随机的、由操作系统分配的端口。
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    // 如果有，拨号给定的多地址表示的节点。
    if let Some(addr) = std::env::args().nth(1) {
        let remote: Multiaddr = addr.parse()?;
        swarm.dial(remote)?;
        println!("拨号 {addr}")
    }

    loop {
        match swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => println!("监听中 {address:?}"),
            // 打印身份信息正在发送到的对等 ID。
            SwarmEvent::Behaviour(identify::Event::Sent { peer_id, .. }) => {
                println!("发送身份信息给 {peer_id:#?}")
            }
            // 打印通过身份事件接收到的信息。
            SwarmEvent::Behaviour(identify::Event::Received { info, .. }) => {
                println!("接收到 {info:?}");
                let info_addr=info.listen_addrs;
                println!("接收到 {info_addr:#?}")
            }

            _ => {}
        }
    }
}
