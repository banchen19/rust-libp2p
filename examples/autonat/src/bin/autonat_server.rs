// Copyright 2021 Protocol Labs.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

#![doc = include_str!("../../README.md")]

use async_std::io;
// 导入所需的库
use clap::Parser;
use futures::prelude::*;
use libp2p::core::{multiaddr::Protocol, upgrade::Version, Multiaddr, Transport};
use libp2p::swarm::{NetworkBehaviour, SwarmBuilder, SwarmEvent};
use libp2p::{autonat, identify, identity, noise, tcp, yamux, PeerId};
use std::error::Error;
use std::net::Ipv4Addr;

// 命令行参数的解析结构体
#[derive(Debug, Parser)]
#[clap(name = "libp2p autonat")]
struct Opt {
    #[clap(long)]
    listen_port: Option<u16>,
}

// 异步主函数
//cargo run --bin autonat_server -- --listen-port 12851
#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // 初始化日志记录器
    env_logger::init();

    // 解析命令行参数
    let opt = Opt::parse();

    // 生成本地密钥对和对应的 PeerId
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("Local peer id: {local_peer_id:?}");

    // 配置 libp2p 传输层
    let transport = tcp::async_io::Transport::default()
        .upgrade(Version::V1Lazy)
        .authenticate(noise::Config::new(&local_key)?)
        .multiplex(yamux::Config::default())
        .boxed();

    // 创建自定义行为
    let behaviour = Behaviour::new(local_key.public());

    // 创建 Swarm
    let mut swarm =
        SwarmBuilder::with_async_std_executor(transport, behaviour, local_peer_id).build();

    // 监听指定地址
    swarm.listen_on(
        Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::UNSPECIFIED))
            .with(Protocol::Tcp(opt.listen_port.unwrap_or(0))),
    )?;

        
    // 事件循环
    loop {
        match swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => println!("Listening on {address:?}"),
            SwarmEvent::Behaviour(event) => println!("{event:?}"),
            e => println!("{e:?}"),
        }
    }
}

// 自定义行为结构体
#[derive(NetworkBehaviour)]
struct Behaviour {
    identify: identify::Behaviour,
    auto_nat: autonat::Behaviour,
}

impl Behaviour {
    // 构造函数，创建自定义行为实例
    fn new(local_public_key: identity::PublicKey) -> Self {
        Self {
            identify: identify::Behaviour::new(identify::Config::new(
                "/ipfs/0.1.0".into(),
                local_public_key.clone(),
            )),
            auto_nat: autonat::Behaviour::new(
                local_public_key.to_peer_id(),
                autonat::Config {
                    only_global_ips: false,
                    ..Default::default()
                },
            ),
        }
    }
}

// 自定义事件枚举
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum Event {
    AutoNat(autonat::Event),
    Identify(identify::Event),
}

impl From<identify::Event> for Event {
    fn from(v: identify::Event) -> Self {
        Self::Identify(v)
    }
}

impl From<autonat::Event> for Event {
    fn from(v: autonat::Event) -> Self {
        Self::AutoNat(v)
    }
}