// 版权 2020 Parity Technologies (UK) Ltd.
//
// 根据本软件及其相关文档文件 ("软件")，授予任何获得该软件副本的人免费使用权，以便在不受限制的情况下处理该软件，包括但不限于使用、复制、修改、合并、发布、分发、再许可和/或销售该软件的副本，并允许接收该软件的人根据以下条件进行操作：
//
// 上述版权声明和本许可声明应包含在所有副本或重要部分的软件中。
//
// 本软件"按原样"提供，无论是明示、暗示，包括但不限于商业使用、适销性、特定用途适用性和非侵权性的任何形式的担保，均不提供担保。
// 作者或版权持有人在任何情况下都无权对任何索赔、损害或其他责任进行赔偿，无论是在合同、侵权还是其他方面，起源于、由于或与本软件或其使用或其他操作有关。

#![doc = include_str!("../README.md")]

use async_std::io;
use either::Either;
use futures::{prelude::*, select};
use libp2p::{
    core::{muxing::StreamMuxerBox, transport, transport::upgrade::Version},
    gossipsub, identify, identity,
    multiaddr::Protocol,
    noise, ping,
    pnet::{PnetConfig, PreSharedKey},
    swarm::{NetworkBehaviour, SwarmBuilder, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, Transport,
};
use std::{env, error::Error, fs, path::Path, str::FromStr, time::Duration};

/// 构建作为所有连接的共同基础的传输。
pub fn build_transport(
    key_pair: identity::Keypair,
    psk: Option<PreSharedKey>,
) -> transport::Boxed<(PeerId, StreamMuxerBox)> {
    let noise_config = noise::Config::new(&key_pair).unwrap();
    let yamux_config = yamux::Config::default();

    let base_transport = tcp::async_io::Transport::new(tcp::Config::default().nodelay(true));
    
    let maybe_encrypted = match psk {
        Some(psk) => Either::Left(
            base_transport.and_then(move |socket, _| PnetConfig::new(psk).handshake(socket)),
        ),
        None => Either::Right(base_transport),
    };

    maybe_encrypted
        .upgrade(Version::V1Lazy)
        .authenticate(noise_config)
        .multiplex(yamux_config)
        .timeout(Duration::from_secs(20))
        .boxed()
}

/// 获取当前的 IPFS 存储库路径，可以从 IPFS_PATH 环境变量或默认的 $HOME/.ipfs 中获取。
fn get_ipfs_path() -> Box<Path> {
    env::var("IPFS_PATH")
        .map(|ipfs_path| Path::new(&ipfs_path).into())
        .unwrap_or_else(|_| {
            env::var("HOME")
                .map(|home| Path::new(&home).join(".ipfs"))
                .expect("无法确定家目录")
                .into()
        })
}

/// 从给定的 IPFS 目录中读取预共享密钥文件。
fn get_psk(path: &Path) -> std::io::Result<Option<String>> {
    let swarm_key_file = path.join("swarm.key");
    match fs::read_to_string(swarm_key_file) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// 对于以对等节点 ID 结尾的多地址，此函数会将其后缀去除。
/// Rust-libp2p 仅支持拨号到不包含对等节点 ID 的地址。
fn strip_peer_id(addr: &mut Multiaddr) {
    let last = addr.pop();
    match last {
        Some(Protocol::P2p(peer_id)) => {
            let mut addr = Multiaddr::empty();
            addr.push(Protocol::P2p(peer_id));
            println!(
                "移除对等节点 ID {addr} 以便 rust-libp2p 可以拨号",
                addr = addr
            );
        }
        Some(other) => addr.push(other),
        _ => {}
    }
}

/// 解析传统的多地址（将 "ipfs" 替换为 "p2p"），并去除对等节点 ID，以便 rust-libp2p 可以拨号。
fn parse_legacy_multiaddr(text: &str) -> Result<Multiaddr, Box<dyn Error>> {
    let sanitized = text
        .split('/')
        .map(|part| if part == "ipfs" { "p2p" } else { part })
        .collect::<Vec<_>>()
        .join("/");
    let mut res = Multiaddr::from_str(&sanitized)?;
    strip_peer_id(&mut res);
    Ok(res)
}

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let ipfs_path = get_ipfs_path();
    println!("使用 IPFS_PATH {ipfs_path:?}");
    let psk: Option<PreSharedKey> = get_psk(&ipfs_path)?
        .map(|text| PreSharedKey::from_str(&text))
        .transpose()?;

    // 创建一个随机的 PeerId
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("使用随机生成的对等节点 ID: {local_peer_id:?}");
    if let Some(psk) = psk {
        println!("使用具有指纹的 swarm 密钥: {}", psk.fingerprint());
    }

    // 设置一个经过加密的、启用了 DNS 的 TCP 传输和 Yamux 协议
    let transport = build_transport(local_key.clone(), psk);

    // 创建一个 Gossipsub 主题
    let gossipsub_topic = gossipsub::IdentTopic::new("chat");

    // 我们创建一个自定义的网络行为，将 gossipsub、ping 和 identify 结合在一起。
    #[derive(NetworkBehaviour)]
    #[behaviour(to_swarm = "MyBehaviourEvent")]
    struct MyBehaviour {
        gossipsub: gossipsub::Behaviour,
        identify: identify::Behaviour,
        ping: ping::Behaviour,
    }

    enum MyBehaviourEvent {
        Gossipsub(gossipsub::Event),
        Identify(identify::Event),
        Ping(ping::Event),
    }

    impl From<gossipsub::Event> for MyBehaviourEvent {
        fn from(event: gossipsub::Event) -> Self {
            MyBehaviourEvent::Gossipsub(event)
        }
    }

    impl From<identify::Event> for MyBehaviourEvent {
        fn from(event: identify::Event) -> Self {
            MyBehaviourEvent::Identify(event)
        }
    }

    impl From<ping::Event> for MyBehaviourEvent {
        fn from(event: ping::Event) -> Self {
            MyBehaviourEvent::Ping(event)
        }
    }

    // 创建一个 Swarm 来管理对等节点和事件
    let mut swarm = {
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .max_transmit_size(262144)
            .build()
            .expect("有效的配置");
        let mut behaviour = MyBehaviour {
            gossipsub: gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(local_key.clone()),
                gossipsub_config,
            )
            .expect("有效的配置"),
            identify: identify::Behaviour::new(identify::Config::new(
                "/ipfs/0.1.0".into(),
                local_key.public(),
            )),
            ping: ping::Behaviour::new(ping::Config::new()),
        };

        println!("订阅 {gossipsub_topic:?}");
        behaviour.gossipsub.subscribe(&gossipsub_topic).unwrap();
        SwarmBuilder::with_async_std_executor(transport, behaviour, local_peer_id).build()
    };

    // 如果指定了其他节点，则建立联系
    for to_dial in std::env::args().skip(1) {
        let addr: Multiaddr = parse_legacy_multiaddr(&to_dial)?;
        swarm.dial(addr)?;
        println!("拨号给 {to_dial:?}")
    }

    // 从标准输入中读取完整行
    let mut stdin = io::BufReader::new(io::stdin()).lines().fuse();

    // 在所有接口上侦听和操作系统分配的任何端口
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    // 启动
    loop {
        select! {
            line = stdin.select_next_some() => {
                if let Err(e) = swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish(gossipsub_topic.clone(), line.expect("预期标准输入不会关闭").as_bytes())
                {
                    println!("发布错误: {e:?}");
                }
            },
            event = swarm.select_next_some() => {
                match event {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        println!("正在监听 {address:?}");
                    }
                    SwarmEvent::Behaviour(MyBehaviourEvent::Identify(event)) => {
                        println!("识别: {event:#?}");
                    }
                    SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                        propagation_source: peer_id,
                        message_id: id,
                        message,
                    })) => {
                        println!(
                            "收到消息: {}，ID: {}，来自对等节点: {:?}",
                            String::from_utf8_lossy(&message.data),
                            id,
                            peer_id
                        )
                    }
                    SwarmEvent::Behaviour(MyBehaviourEvent::Ping(event)) => {
                        match event {
                            ping::Event {
                                peer,
                                result: Result::Ok(rtt),
                                ..
                            } => {
                                println!(
                                    "ping: 到 {} 的往返时间是 {} 毫秒",
                                    peer.to_base58(),
                                    rtt.as_millis()
                                );
                            }
                            ping::Event {
                                peer,
                                result: Result::Err(ping::Failure::Timeout),
                                ..
                            } => {
                                println!("ping: 到 {} 的超时", peer.to_base58());
                            }
                            ping::Event {
                                peer,
                                result: Result::Err(ping::Failure::Unsupported),
                                ..
                            } => {
                                println!("ping: {} 不支持 ping 协议", peer.to_base58());
                            }
                            ping::Event {
                                peer,
                                result: Result::Err(ping::Failure::Other { error }),
                                ..
                            } => {
                                println!("ping: ping::Failure with {}: {error}", peer.to_base58());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
