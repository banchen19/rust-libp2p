// 版权所有 2021 Protocol Labs。
//
// 在此授予任何获得本软件及其相关文档文件（以下简称“软件”）副本的人，无需支付费用，
// 以任何方式处理本软件，包括但不限于使用、复制、修改、合并、发布、分发、再许可和/或销售软件的副本，
// 并允许将软件提供给接收者，但须遵守以下条件：
//
// 上述版权声明和本许可声明应包含在所有副本或实质部分的软件中。
//
// 本软件按“原样”提供，无任何明示或暗示的保证，
// 包括但不限于适销性、特定用途适用性和非侵权性保证。作者或版权所有人在任何情况下均不承担任何索赔、损害赔偿或其他责任，
// 无论是因合同、侵权还是其他方式引起的、与本软件或使用或其他方式相关的，包括但不限于软件的使用或其他方式产生的
// 从、与或在本软件或其使用或其他方式中产生的其他责任。

#![doc = include_str!("../../README.md")]

use async_std::io;
use clap::Parser;
use futures::{prelude::*, select};
use libp2p::core::multiaddr::Protocol;
use libp2p::core::{upgrade::Version, Multiaddr, Transport};
use libp2p::swarm::{NetworkBehaviour, SwarmBuilder, SwarmEvent};
use libp2p::{autonat, identify, identity, noise, tcp, yamux, PeerId};
use std::error::Error;
use std::net::Ipv4Addr;
use std::time::Duration;
// 命令行参数的结构体定义
#[derive(Debug, Parser)]
#[clap(name = "libp2p autonat")]
struct Opt {
    #[clap(long)]
    listen_port: Option<u16>,

    #[clap(long)]
    server_address: Multiaddr,

    #[clap(long)]
    server_peer_id: PeerId,
}

//cargo run --bin autonat_client -- --server-address /ip4/123.157.164.217/tcp/12851
// --server-peer-id 12D3KooWED5qBVkso8P7EGzxUuwSmpATjRX8Fs5JFsvYV861aXfa --listen-port 12345
#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let opt: Opt = Opt::parse();

    // 生成本地密钥对
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("Local peer id: {local_peer_id:?}");

    // 配置传输协议
    let transport = tcp::async_io::Transport::default()
        .upgrade(Version::V1Lazy)
        .authenticate(noise::Config::new(&local_key)?)
        .multiplex(yamux::Config::default())
        .boxed();

    // 创建行为组合
    let behaviour = Behaviour::new(local_key.public());

    // 创建 Swarm 实例
    let mut swarm =
        SwarmBuilder::with_async_std_executor(transport, behaviour, local_peer_id).build();

    // 监听指定地址
    swarm.listen_on(
        Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::UNSPECIFIED))
            .with(Protocol::Tcp(opt.listen_port.unwrap_or(0))),
    )?;

    // 向自动NAT服务器添加信息
    swarm
        .behaviour_mut()
        .auto_nat
        .add_server(opt.server_peer_id, Some(opt.server_address));

    // 从标准输入中读取完整行
    let mut stdin = io::BufReader::new(io::stdin()).lines().fuse();

    // 事件循环
    loop {
        match swarm.select_next_some().await {
            // 监听地址变化事件
            SwarmEvent::NewListenAddr { address, .. } => println!("Listening on {address:?}"),
            // 行为事件
            SwarmEvent::Behaviour(event) => println!("{event:?}"),
            e => println!("{e:?}"),
        }
    }
}

// 定义行为组合
#[derive(NetworkBehaviour)]
struct Behaviour {
    identify: identify::Behaviour,
    auto_nat: autonat::Behaviour,
}

impl Behaviour {
    // 构造函数
    fn new(local_public_key: identity::PublicKey) -> Self {
        Self {
            // 创建 identify 行为
            identify: identify::Behaviour::new(identify::Config::new(
                "/ipfs/0.1.0".into(),
                local_public_key.clone(),
            )),
            // 创建自动NAT行为
            auto_nat: autonat::Behaviour::new(
                local_public_key.to_peer_id(),
                autonat::Config {
                    retry_interval: Duration::from_secs(10),
                    refresh_interval: Duration::from_secs(30),
                    boot_delay: Duration::from_secs(5),
                    throttle_server_period: Duration::ZERO,
                    only_global_ips: false,
                    ..Default::default()
                },
            ),
        }
    }
}

// 定义事件枚举类型
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum Event {
    AutoNat(autonat::Event),
    Identify(identify::Event),
}

// 实现从 identify::Event 到 Event 的转换
impl From<identify::Event> for Event {
    fn from(v: identify::Event) -> Self {
        Self::Identify(v)
    }
}

// 实现从 autonat::Event 到 Event 的转换
impl From<autonat::Event> for Event {
    fn from(v: autonat::Event) -> Self {
        Self::AutoNat(v)
    }
}
