# WorldLinkd 系统架构与协议文档

## 1. 系统概述

WorldLinkd 是一个游戏网络中继服务，主要用于 Futari 游戏客户端之间的联机通信。该系统由两个主要组件组成：

1. **FutariRelay** - 网络中继服务，负责客户端间的通信
2. **FutariLobby** - HTTP API 服务，提供游戏匹配和招募管理功能

## 2. 系统架构

### 2.1 整体架构

```
+----------------+      HTTP      +----------------+
|                |  (Port 20100)  |                |
| 游戏客户端       | <-----------> | FutariLobby    |
|                |                | (匹配服务)      |
+----------------+                +----------------+
        |                                 |
        |                                 |
        | TCP Socket                      | 共享数据
        |                                 |
        v                                 v
+----------------+                +----------------+
|                |                |                |
| FutariRelay    | <-----------> | 招募记录存储     |
| (网络中继服务)   |                | (ConcurrentMap)|
+----------------+                +----------------+
```

### 2.2 主要组件

#### 2.2.1 Application.kt

- 系统入口点
- 启动 Netty HTTP 服务器 (端口 20100)
- 启动 FutariRelay 服务

#### 2.2.2 FutariRelay

- 提供客户端之间的通信中继
- 处理客户端注册、消息转发和连接管理
- 支持 TCP 流多路复用

#### 2.2.3 FutariLobby

- 提供 HTTP API 接口
- 处理游戏招募管理和匹配功能
- 维护活跃招募记录

#### 2.2.4 数据结构

- FutariTypes.kt 定义核心数据类型
- 使用 Kotlinx Serialization 进行序列化

#### 2.2.5 日志系统

- `AquaLogging` 提供彩色终端日志输出

## 3. 中继协议 (FutariRelay)

### 3.1 连接流程

1. 客户端通过 TCP 连接到中继服务
2. 客户端发送 `CTL_START` 命令进行注册
3. 服务器创建 `ActiveClient` 实例并返回版本信息
4. 客户端可以开始发送/接收消息

### 3.2 消息格式

消息由逗号分隔的字符串表示，格式如下：

```
1,<cmd>,<proto>,<sid>,<src>,<sPort>,<dst>,<dPort>,,,,,,,,,<data>
```

字段说明：

- `1` - 固定值
- `cmd` - 命令 ID (UInt)
- `proto` - 协议类型 (TCP=6, UDP=17)
- `sid` - 流 ID (UInt)
- src - 源 IP 地址 (UInt)
- `sPort` - 源端口 (UInt)
- `dst` - 目标 IP 地址 (UInt)
- `dPort` - 目标端口 (UInt)
- 8 个保留字段 (逗号占位)
- `data` - 可选数据负载 (String)

### 3.3 命令类型

#### 控制平面命令

- `CTL_START` (1u) - 客户端注册

  - 数据: 客户端 ID (keychip)
  - 响应: 带有版本信息的 `CTL_START` 消息

- `CTL_BIND` (2u) - 绑定
- `CTL_HEARTBEAT` (3u) - 心跳检测

- `CTL_TCP_CONNECT` (4u) - 请求新的 TCP 流

  - 需要 `sid` (流 ID)
  - 服务器将流添加到待处理列表

- `CTL_TCP_ACCEPT` (5u) - 接受 TCP 连接

  - 需要 `sid` (流 ID)
  - 服务器将流从待处理移到活动列表

- `CTL_TCP_ACCEPT_ACK` (6u) - 连接接受确认

- `CTL_TCP_CLOSE` (7u) - 关闭 TCP 流

#### 数据平面命令

- `DATA_SEND` (21u) - 发送数据到特定目标

  - 服务器将消息转发到目标客户端

- `DATA_BROADCAST` (22u) - 广播数据到所有客户端
  - 仅支持 UDP 协议

### 3.4 TCP 流管理

系统支持 TCP 流多路复用：

1. 客户端发送 `CTL_TCP_CONNECT` 创建新流
2. 服务器将流 ID 添加到待处理列表 (`pendingStreams`)
3. 目标客户端发送 `CTL_TCP_ACCEPT`
4. 服务器将流移动到活动列表 (`tcpStreams`)
5. 流量通过 `DATA_SEND` 消息在流上传输

每个客户端最多允许 `MAX_STREAMS` (10) 个待处理流。

## 4. Lobby API (HTTP)

### 4.1 基础信息

- 端口: 20100
- 格式: JSON
- 序列化: Kotlinx JSON

### 4.2 API 端点

#### GET /

- 描述: 健康检查
- 响应: "Running!"

#### POST /recruit/start

- 描述: 开始一个新招募
- 请求体: `RecruitRecord`
- 响应: 200 OK
- 行为: 将招募记录添加到招募记录表中

#### POST /recruit/finish

- 描述: 完成招募
- 请求体: `RecruitRecord`
- 响应: 200 OK 或 404 (如未找到)
- 行为: 更新招募状态为已完成

#### GET /recruit/list

- 描述: 获取所有活跃招募
- 响应: `RecruitRecord` 对象列表

#### GET /recruit/clean

- 描述: 清理过期招募
- 响应: 200 OK
- 行为: 移除超过 MAX_TTL (30 秒) 的招募记录

#### GET /debug

- 描述: 返回调试信息
- 响应: 包含各种系统信息的 JSON

## 5. 关键数据结构

### 5.1 招募系统

```kotlin
// 招募记录
data class RecruitRecord(
    val RecruitInfo: RecruitInfo,    // 招募信息
    val Keychip: String,             // 客户端唯一标识
    var Server: RelayServerInfo?,    // 服务器信息
    var Time: Long                   // 时间戳
)

// 招募信息
data class RecruitInfo(
    val MechaInfo: MechaInfo,        // 机器信息
    val MusicID: Int,                // 音乐ID
    val GroupID: Int,                // 组ID
    val EventModeID: Boolean,        // 事件模式ID
    val JoinNumber: Int,             // 加入人数
    val PartyStance: Int,            // 队伍立场
    val _startTimeTicks: Long,       // 开始时间
    val _recvTimeTicks: Long         // 接收时间
)

// 机器信息
data class MechaInfo(
    val IsJoin: Boolean,             // 是否加入
    val IpAddress: UInt,             // IP地址
    val MusicID: Int,                // 音乐ID
    val Entrys: List<Boolean>,       // 参与情况
    var UserIDs: List<Long>,         // 用户ID列表
    val UserNames: List<String>,     // 用户名列表
    val IconIDs: List<Int>,          // 图标ID列表
    val FumenDifs: List<Int>,        // 谱面难度列表
    val Rateing: List<Int>,          // 评级列表
    val ClassValue: List<Int>,       // 等级值列表
    val MaxClassValue: List<Int>,    // 最大等级值列表
    val UserType: List<Int>          // 用户类型列表
)
```

### 5.2 中继系统

```kotlin
// 活跃客户端
data class ActiveClient(
    val clientKey: String,           // 客户端密钥
    val socket: Socket,              // 套接字
    val reader: BufferedReader,      // 读取器
    val writer: BufferedWriter,      // 写入器
    val thread: Thread,              // 线程
    val tcpStreams: MutableMap<UInt, UInt>,    // <流ID, 目标客户端IP>
    val pendingStreams: MutableSet<UInt>       // 待处理流ID集合
)

// 消息
data class Msg(
    var cmd: UInt,                   // 命令
    var proto: UInt?,                // 协议
    var sid: UInt?,                  // 流ID
    var src: UInt?,                  // 源IP
    var sPort: UInt?,                // 源端口
    var dst: UInt?,                  // 目标IP
    var dPort: UInt?,                // 目标端口
    var data: String?                // 数据
)
```

## 6. 实现服务端的步骤

要实现一个完整的服务端，请按照以下步骤：

1. **创建基础结构**

   - 设置 Netty HTTP 服务器 (端口 20100)
   - 实现 FutariRelay TCP 服务器
   - 实现日志系统

2. **实现中继服务**

   - 客户端连接处理
   - 客户端注册 (CTL_START)
   - 消息解析与转发
   - TCP 流管理

3. **实现 HTTP API**

   - 招募开始/完成接口
   - 招募列表查询
   - 招募清理接口

4. **数据管理**

   - 实现招募记录存储
   - 处理客户端连接映射

5. **错误处理**

   - 实现超时处理
   - 处理连接错误
   - 日志记录

6. **启动流程**
   - 实现 `main()` 函数启动所有服务
   - 确保适当的资源清理

按照此架构和协议文档，您应该能够实现一个与现有系统兼容的服务端。
