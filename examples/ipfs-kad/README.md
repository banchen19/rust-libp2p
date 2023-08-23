## 描述

本示例展示了如何使用 **libp2p** 在 IPFS 网络上与 Kademlia 协议进行交互。
代码演示了如何执行 Kademlia 查询以查找最接近特定对等节点 ID 的对等节点。
通过运行此示例，用户可以更好地了解 Kademlia 协议在 IPFS 网络上的操作方式以及如何执行查询。

## 使用方法

示例代码演示了如何使用 Rust P2P 库在 IPFS 网络上执行 Kademlia 查询。
通过将对等节点 ID 指定为参数，代码将搜索与给定对等节点 ID 最接近的对等节点。

### 参数

运行示例代码：



```sh
cargo run [PEER_ID]
```

将 `[PEER_ID]` 替换为您要搜索的以 base58 编码的对等节点 ID。
如果未提供对等节点 ID，则将生成一个随机对等节点 ID。



## 示例输出

运行示例代码后，您将在控制台中看到输出。
输出将显示 Kademlia 查询的结果，包括最接近指定对等节点 ID 的对等节点。

### 成功查询输出

如果 Kademlia 查询成功找到最接近的对等节点，则输出为：

```sh
Searching for the closest peers to [PEER_ID]
Query finished with closest peers: [peer1, peer2, peer3]
```


### 失败查询输出

如果 Kademlia 查询超时或没有可达的对等节点，则输出将指示失败：



```sh
Searching for the closest peers to [PEER_ID]
Query finished with no closest peers.
```


## 结论

总之，本示例实际演示了如何使用 Rust P2P 库与 IPFS 网络上的 Kademlia 协议进行交互。
通过检查代码并运行示例，用户可以深入了解 Kademlia 的内部工作原理以及它如何执行查询以查找最接近的对等节点。
这些知识在开发点对点应用程序或理解分散式网络时可能非常有价值。
