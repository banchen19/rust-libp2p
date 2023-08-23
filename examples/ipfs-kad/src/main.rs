// 版权所有 2018 Parity Technologies (UK) Ltd.
//
// 特此授予任何获得此软件及相关文档文件（以下称为“软件”）的副本的人，可以免费使用此软件，
// 在不受限制的情况下处理此软件，包括但不限于使用、复制、修改、合并、发布、分发、再许可
// 以及/或出售此软件的副本，并允许获得此软件的人这样做，但需符合以下条件：
//
// 上述版权声明和本许可声明应包含在所有副本或重要部分的软件中。
//
// 本软件按“原样”提供，无任何明示或暗示的保证，
// 包括但不限于适销性、特定用途适用性以及非侵权的保证。在任何情况下，
// 作者或版权持有人均不对任何索赔、损害或其他责任负责，
// 无论是因合同行为、侵权行为还是其他原因导致、与之相关或与之有关的损害或责任，
// 无论是在合同、侵权或其他情况下，即使提前被告知有可能发生此类损害。

#![doc = include_str!("../README.md")]

use futures::StreamExt;
use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{GetClosestPeersError, Kademlia, KademliaConfig, KademliaEvent, QueryResult};
use libp2p::{
    development_transport, identity,
    swarm::{SwarmBuilder, SwarmEvent},
    PeerId,
};
use std::{env, error::Error, time::Duration};

const BOOTNODES: [&str; 4] = [
    "QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
    "QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
    "QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb",
    "QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt",
];

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    // 为自己创建一个随机密钥。
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());

    // 设置一个带有 yamux 协议的加密的 DNS 启用的 TCP 传输。
    let transport = development_transport(local_key).await?;

    // 创建一个 swarm 来管理对等节点和事件。
    let mut swarm = {
        // 创建一个 Kademlia 行为。
        let mut cfg = KademliaConfig::default();
        cfg.set_query_timeout(Duration::from_secs(5 * 60));
        let store = MemoryStore::new(local_peer_id);
        let mut behaviour = Kademlia::with_config(local_peer_id, store, cfg);

        // 将引导节点添加到本地路由表中。嵌入在 `transport` 中的 `libp2p-dns`
        // 将在 Kademlia 尝试拨号这些节点时解析 `dnsaddr`。
        for peer in &BOOTNODES {
            behaviour.add_address(&peer.parse()?, "/dnsaddr/bootstrap.libp2p.io".parse()?);
        }

        SwarmBuilder::with_async_std_executor(transport, behaviour, local_peer_id).build()
    };

    // 命令 Kademlia 搜索对等节点。
    let to_search = env::args()
        .nth(1)
        .map(|p| p.parse())
        .transpose()?
        .unwrap_or_else(PeerId::random);

    println!("正在搜索与 {to_search} 最近的对等节点");
    swarm.behaviour_mut().get_closest_peers(to_search);

    loop {
        let event = swarm.select_next_some().await;
        if let SwarmEvent::Behaviour(KademliaEvent::OutboundQueryProgressed {
            result: QueryResult::GetClosestPeers(result),
            ..
        }) = event
        {
            match result {
                Ok(ok) => {
                    if !ok.peers.is_empty() {
                        println!("查询完成，最近的对等节点: {:#?}", ok.peers)
                    } else {
                        // 如果至少有一个可达的对等节点，示例被视为失败。
                        println!("查询完成，没有最近的对等节点。")
                    }
                }
                Err(GetClosestPeersError::Timeout { peers, .. }) => {
                    if !peers.is_empty() {
                        println!("查询超时，最近的对等节点: {peers:#?}")
                    } else {
                        // 如果至少有一个可达的对等节点，示例被视为失败。
                        println!("查询超时，没有最近的对等节点。");
                    }
                }
            };

            break;
        }
    }

    Ok(())
}
