// 版权 2018 Parity Technologies (UK) Ltd.
//
// 在此授予任何获得此软件及其相关文档文件（以下简称"软件"）副本的人免费的许可，
// 可以无限制地使用、复制、修改、合并、发布、分发、再授权和/或销售软件的副本，
// 并允许在使用软件的人员遵守以下条件：
//
// 上述版权声明和本许可声明必须包含在所有副本或实质部分的软件中。
//
// 软件以"原样"提供，不附带任何形式的明示或暗示的保证，
// 包括但不限于适销性、适用性和非侵权性。在任何情况下，
// 作者或版权持有人均不承担因使用软件或与之相关的其他操作而产生的任何索赔、损害或其他责任，
// 无论是在合同、侵权行为还是其他方面产生的，即使事先已被告知有可能发生此类损害。

#![doc = include_str!("../README.md")]

use async_std::io;
use futures::{future::Either, prelude::*, select};
use libp2p::{
    core::{muxing::StreamMuxerBox, transport::OrTransport, upgrade},
    gossipsub, identity, mdns, noise, quic,
    swarm::NetworkBehaviour,
    swarm::{SwarmBuilder, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, Transport,
};
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::time::Duration;
use std::{collections::hash_map::DefaultHasher, str::FromStr};

use rand::Rng;

// 我们创建一个自定义的网络行为，将 Gossipsub 和 Mdns 结合起来。
#[derive(NetworkBehaviour)]
struct MyBehaviour {
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::async_io::Behaviour,
}

//生成一个随机的key
pub fn generate_random_key(length: usize) -> String {
    let mut rng = rand::thread_rng();
    let characters: Vec<char> = "abcdefghijklmnopqrstuvwxyz0123456789".chars().collect();
    let key: String = (0..length)
        .map(|_| {
            let idx = rng.gen_range(0..characters.len());
            characters[idx]
        })
        .collect();
    key
}

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // 创建一个随机的 PeerId
    let id_keys = identity::Keypair::generate_ed25519();

    let local_peer_id = PeerId::from(id_keys.public());
    println!("Local peer id: {local_peer_id}");

    // 设置加密的启用了 DNS 的 TCP Transport，使用 yamux 协议。
    let tcp_transport = tcp::async_io::Transport::new(tcp::Config::default().nodelay(true))
        // 使用升级模块将传输升级为 V1Lazy 版本
        .upgrade(upgrade::Version::V1Lazy)
        // 使用 libp2p-noise 进行身份验证和加密通信
        .authenticate(noise::Config::new(&id_keys).expect("signing libp2p-noise static keypair"))
        // 使用 yamux 多路复用协议进行数据传输
        .multiplex(yamux::Config::default())
        // 设置传输的超时时间为 20 秒
        .timeout(std::time::Duration::from_secs(3))
        .boxed();

    // 创建 QUIC Transport，使用 libp2p-noise 进行加密通信
    let quic_transport = quic::async_std::Transport::new(quic::Config::new(&id_keys));

    // 使用 OrTransport 将两种传输协议进行组合，根据条件选择其中之一
    let transport = OrTransport::new(quic_transport, tcp_transport)
        // 使用 map 函数对传输结果进行处理，以返回包装的结果
        .map(|either_output, _| match either_output {
            // 对于左侧（QUIC）传输结果，创建新的 StreamMuxerBox 实例
            Either::Left((peer_id, muxer)) => (peer_id, StreamMuxerBox::new(muxer)),
            // 对于右侧（TCP）传输结果，同样创建新的 StreamMuxerBox 实例
            Either::Right((peer_id, muxer)) => (peer_id, StreamMuxerBox::new(muxer)),
        })
        .boxed();

    // 为内容寻址的消息，我们可以对消息的哈希值进行散列，然后使用它作为 ID。
    let message_id_fn = |message: &gossipsub::Message| {
        let mut s = DefaultHasher::new();
        message.data.hash(&mut s);
        // 生成一个随机的key
        // gossipsub::MessageId::from(generate_random_key(16))
        gossipsub::MessageId::from(s.finish().to_string())
    };

    // 设置自定义的 gossipsub 配置
    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(10)) // 这是为了通过不堆积日志来辅助调试
        .validation_mode(gossipsub::ValidationMode::Strict) // 这设置了消息验证的类型。默认为 Strict（强制消息签名）
        .message_id_fn(message_id_fn) // 内容寻址的消息。不会传播相同内容的两条消息。
        .build()
        .expect("有效的配置");

    // 构建一个 gossipsub 网络行为
    let mut gossipsub = gossipsub::Behaviour::new(
        gossipsub::MessageAuthenticity::Signed(id_keys),
        gossipsub_config,
    )
    .expect("正确的配置");

    // 创建一个 Gossipsub 主题
    let topic = gossipsub::IdentTopic::new("test-net");
    // 订阅我们的主题
    gossipsub.subscribe(&topic)?;

    // 创建一个 Swarm 以管理对等节点和事件
    let mut swarm = {
        let mdns = mdns::async_io::Behaviour::new(mdns::Config::default(), local_peer_id)?;
        let behaviour = MyBehaviour { gossipsub, mdns };
        SwarmBuilder::with_async_std_executor(transport, behaviour, local_peer_id).build()
    };

    // 从标准输入读取完整行
    let mut stdin = io::BufReader::new(io::stdin()).lines().fuse();

    // 在所有接口上侦听并使用操作系统分配的任何端口
    swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    //  // 设置远程 Multiaddr
    //  let remote: Multiaddr = "/ip4/127.0.0.1/tcp/35791".parse()?;
    // swarm.dial(remote)?;

    println!("通过标准输入输入消息，它们将使用 Gossipsub 发送到连接的对等节点");

    // 启动
    loop {
        select! {
            line = stdin.select_next_some() => {
                if let Err(e) = swarm
                    .behaviour_mut().gossipsub
                    .publish(topic.clone(), line.expect("Stdin 未关闭").as_bytes()) {
                    println!("发布错误：{e:?}");
                }
            },
            event = swarm.select_next_some() => match event {
                SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                    for (peer_id, _multiaddr) in list {
                        println!("mDNS 发现了新对等节点：{peer_id}");
                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                    }
                },
                SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
                    for (peer_id, _multiaddr) in list {
                        println!("mDNS 发现的对等节点已过期：{peer_id}");
                        // swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                    }
                },
                SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                    propagation_source: peer_id,
                    message_id: id,
                    message,
                })) => println!(
                        "收到消息：'{}'，ID：{id}，来自对等节点：{peer_id}",
                        String::from_utf8_lossy(&message.data),
                    ),
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("本地节点正在侦听 {address}");
                }
                _ => {}
            }
        }
    }
}
