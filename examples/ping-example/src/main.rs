// 包含版权声明和许可证信息
// 这段注释说明了代码的版权和使用许可。根据许可，您可以自由地使用、修改、分发和出售这个软件，
// 但需要在所有副本或重要部分中包含上述版权和许可声明。
#![doc = include_str!("../README.md")]

// 导入所需的 crate 和模块
use futures::prelude::*;
use libp2p::core::upgrade::Version;
use libp2p::{
    identity, noise, ping,
    swarm::{keep_alive, NetworkBehaviour, SwarmBuilder, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, Transport,
};
use std::error::Error;

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // 生成本地节点的密钥对和标识
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("Local peer id: {local_peer_id:?}");

    // 配置传输协议
    let transport = tcp::async_io::Transport::default()
        // 进行版本升级和协议升级
        .upgrade(Version::V1Lazy)
        // 使用本地密钥创建噪声协议配置
        .authenticate(noise::Config::new(&local_key)?)
        // 使用默认的 yamux 配置进行多路复用
        .multiplex(yamux::Config::default())
        // 将配置封装为一个 Boxed 传输
        .boxed();

    // 创建 libp2p Swarm
    let mut swarm =
        SwarmBuilder::with_async_std_executor(transport, Behaviour::default(), local_peer_id)
            .build();

    // 告诉 Swarm 在所有接口上监听并分配一个随机的操作系统分配的端口
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    // 如果命令行参数中提供了要连接的节点的地址，则尝试拨号连接
    if let Some(addr) = std::env::args().nth(1) {
        let remote: Multiaddr = addr.parse()?;
        swarm.dial(remote)?;
        println!("Dialed {addr}")
    }

    // 进入事件循环，处理 Swarm 事件
    loop {
        match swarm.select_next_some().await {
            // 处理新的监听地址事件
            SwarmEvent::NewListenAddr { address, .. } => println!("Listening on {address:?}"),

            // 处理网络行为事件
            SwarmEvent::Behaviour(event) => println!("{event:?}"),
            _ => {}
        }
    }
}

/// 我们的网络行为。
///
/// 为了演示目的，这里包括 [`KeepAlive`](keep_alive::Behaviour) 行为，以便可以观察到连续的 ping 序列。
#[derive(NetworkBehaviour, Default)]
struct Behaviour {
    keep_alive: keep_alive::Behaviour,
    ping: ping::Behaviour,
}
