# DogBot `claude-runner` 内置 Bifrost 单模型锁定设计

## 目标

本次设计把模型选择和模型锁定职责从宿主机 `agent-runner` 移到 `claude-runner` 容器内的 `Bifrost`，目标是：

- 在 `claude-runner` 容器内引入常驻 `Bifrost` 进程
- 让 `Claude Code` 只访问同容器内的 `Bifrost`
- 由 `Bifrost` 负责协议转换、上游模型选择和单模型锁定
- 删除 `agent-runner` 的 `API_PROXY_UPSTREAM_MODEL` 以及请求体 `model` 改写逻辑
- 把“切换模型”收敛为部署配置变更，而不是运行时进入容器手动改 Claude 默认模型

本次设计明确选择“方案一”：

- 不做 token split
- 不做 sidecar
- 不把当前方案定义成对抗式强安全边界

也就是说，这次要解决的是“部署与运行路径的稳定锁定”，不是“恶意容器内进程无法绕过”的硬隔离。

## 当前取舍原因与 Trade-off

本次明确选择“同容器 `Bifrost` + 单模型软锁定”，不是因为它在所有维度都最优，而是因为它在当前项目阶段最合适。

当前取舍原因：

- 当前项目更缺“稳定、可复现、可部署的模型选择入口”，而不是对抗式安全隔离
- 现有 `claude-runner` 已经是长期存活的运行容器，把 `Bifrost` 放进去可以最少改动地接入
- 当前最痛的问题是“模型配置散落在 Claude 默认设置和宿主机 rewrite 里”，而不是“容器内 agent 一定会主动攻击自己的网关”
- 项目已经接受 Docker 内 CLI agent 能访问外网，因此当前威胁模型并没有把 `Claude Code` 定义成必须严防死守的不可信对手

这次方案的收益：

- 模型选择入口统一收敛到部署配置
- 不再依赖 `agent-runner` 的协议特定 `model` 覆盖逻辑
- `Claude Code` 和 `Bifrost` 同生共死，部署路径简单
- 后续接入 `gpt`、`gemini`、`deepseek` 等模型时，结构更自然

这次方案付出的代价：

- 安全边界不是硬锁，只是运行路径上的软锁定
- `Bifrost` 和 `Claude Code` 共用一个容器资源配额
- 容器内仍然存在绕过 `Bifrost` 直连宿主机 proxy 的理论可能
- 容器启动逻辑会从“单纯保活”变成“带本地网关守护”，排障复杂度会上升一点

为什么不选更强的方案：

- 不选 token split
  - 因为它会把当前工作从“模型控制面收敛”扩大到“容器内认证边界重构”
- 不选 sidecar
  - 因为它会引入新的 compose 服务、日志面和生命周期协同，超出当前问题的最小解
- 不选继续保留 `API_PROXY_UPSTREAM_MODEL`
  - 因为这会让宿主机 proxy 继续承担 provider/model 语义，和本次收敛方向相反

因此本次设计的明确 trade-off 是：

- 用较弱的安全隔离，换取更低的实现复杂度和更顺的运维路径
- 先把“模型选择与锁定职责”放到正确层级，再决定未来是否继续强化安全边界

## 背景与问题

当前链路是：

```text
Claude Code -> host.docker.internal:9000(agent-runner api-proxy) -> 上游模型服务
```

现状有四个问题：

1. `agent-runner` 当前通过 `API_PROXY_UPSTREAM_MODEL` 覆盖 `/v1/messages` 请求体里的 `model` 字段，这让宿主机 proxy 直接承担了 provider/model 语义，边界偏脆弱。
2. 当前“锁模型”主要依赖宿主机 env 或进入容器后手动改 Claude 默认模型，运维体验不稳定，也不够可复现。
3. `Claude Code` 的模型体验和上游 provider 绑定在一起，未来如果想跑 `gpt`、`gemini`、`deepseek` 等非 Claude 模型，当前结构不自然。
4. 现在没有一个明确的、面向容器生命周期的“单模型锁定”落点。

因此需要把模型相关语义从 `agent-runner` 身上卸掉，收敛到更接近 CLI Agent 的地方，也就是 `claude-runner` 容器内。

## 设计结果

最终运行链路调整为：

```text
QQ / WeChat Adapter
-> agent-runner
-> claude-runner 容器
   -> Claude Code
   -> 127.0.0.1:<BIFROST_PORT>/anthropic
   -> Bifrost
-> host.docker.internal:9000(agent-runner api-proxy)
-> 真实上游模型服务
```

新的职责边界如下：

- `Claude Code`
  - 只把请求发到同容器内的 `Bifrost`
  - 不再作为模型选择的可信控制面
- `Bifrost`
  - 负责 Anthropic 兼容入站
  - 负责 provider 协议转换
  - 负责唯一模型选择
  - 负责单模型锁定
- `agent-runner api-proxy`
  - 只做本地鉴权、上游鉴权头注入和通用 HTTP 转发
  - 不再理解或改写 `model`

## 明确约束

本次设计的约束必须写死：

- 运行时不支持 `/model` 切换
- 运行时不依赖 `Claude Code` 自己的默认模型配置来做锁定
- 变更模型只能修改部署配置并重启后生效
- `agent-runner` 不再提供 `API_PROXY_UPSTREAM_MODEL`
- 模型锁定必须由 `Bifrost` 自身负责，而不是由宿主机 proxy “覆盖重写”

## 非目标

本次不解决以下问题：

- 不保证恶意的容器内进程绝对无法绕过 `Bifrost`
- 不引入新的 sidecar 容器
- 不增加额外的宿主机防火墙或 egress 白名单
- 不做 `Claude -> Bifrost` 与 `Bifrost -> agent-runner` 两段独立 token
- 不做多模型白名单切换

后续如果安全要求提高，可以再考虑：

- token split
- `Bifrost` sidecar
- `claude-runner` 到宿主机的端口级限制
- 宿主机 proxy 只接受来自 `Bifrost` 的独立凭证

## 为什么选择同容器 Bifrost

本次不选 sidecar，而是选“同容器 `Bifrost`”，原因是：

- `claude-runner` 本来就是一个长期存活的 CLI 容器，适合附带常驻本地网关
- `Bifrost` 和 `Claude Code` 同生共死，部署和回收路径简单
- 当前镜像已经包含 Node.js，适合直接在镜像里安装并启动 `Bifrost`
- 不需要为当前仓库额外引入新的 compose 服务拓扑

相比 sidecar，同容器方案的安全边界更弱，但运维路径更短，更符合当前项目“先把路径稳定跑通”的目标。

## 模型锁定语义

### 锁定原则

锁定后的系统语义应当是：

- 每个部署实例只允许一个目标模型
- 这个目标模型由部署配置决定
- 这个目标模型由 `Bifrost` 在启动时读取并固定
- 运行中的 `Claude Code` 即使修改自己的环境变量，也不会改变已经启动的 `Bifrost`
- 如果 `Bifrost` 被杀掉，请求链路直接失败，不允许自动回落到别的直连路径

### 锁定粒度

这里的“锁定”不是锁定 Claude 的 `sonnet/opus/haiku` tier，而是直接锁定到一个确定的 provider-qualified model。

例如：

- `openai/gpt-5`
- `gemini/gemini-2.5-pro`
- `anthropic/claude-sonnet-4-5-20250929`

DogBot 对外只暴露一个锁定模型配置，不再暴露多 tier 模型映射。

### 为什么不依赖 Claude Code 自己的模型配置

`Claude Code` 自己支持环境变量和 `/model` 切换，但这些能力适合“可切换模型”的交互式使用，不适合作为 DogBot 的最终锁定机制。

DogBot 需要的是：

- 可部署
- 可复现
- 可重启恢复
- 不依赖人工进入容器操作

因此本次设计不把 Claude 自己的 model env 当成真正锁定点，只把 `Bifrost` 当成唯一模型控制面。

## 进程与生命周期设计

### 镜像层

`docker/claude-runner/Dockerfile` 需要新增 `Bifrost` 安装。

当前镜像已经安装 Node.js，因此实现上直接采用官方推荐的 Node 运行方式：

```text
npx -y @maximhq/bifrost
```

这避免额外引入 Go 工具链或单独二进制分发流程。

### 启动层

`docker/claude-runner/entrypoint.sh` 不再只是 `sleep infinity`，而是改成：

1. 执行现有 bootstrap
2. 生成本地 `Bifrost` 启动配置
3. 启动 `Bifrost`
4. 等待 `Bifrost` 健康可用
5. 维持容器主进程
6. 如果 `Bifrost` 异常退出，则容器主进程退出

这样做的目的有两个：

- 容器是否可用由 `Bifrost` 是否可用决定
- 如果 `Claude Code` 或其他容器内进程误杀 `Bifrost`，不会继续保留一个“看起来活着但实际上链路已断”的空壳容器

### 运行时路径

`agent-runner` 进入容器执行 `claude --print ...` 时，`Claude Code` 看到的 `ANTHROPIC_BASE_URL` 必须是同容器内 `Bifrost` 地址，而不是宿主机 proxy 地址。

例如：

```text
ANTHROPIC_BASE_URL=http://127.0.0.1:8081/anthropic
ANTHROPIC_AUTH_TOKEN=<bifrost-local-token>
```

而 `Bifrost` 自己的上游则指向：

```text
http://host.docker.internal:9000
```

这让默认运行路径天然经过 `Bifrost`。

## 配置面设计

DogBot 需要新增一组明确的容器内 `Bifrost` 配置，而不是继续把模型相关配置塞给 `agent-runner api-proxy`。

建议新增以下部署配置：

- `BIFROST_PORT`
  - 容器内 `Bifrost` 监听端口
- `BIFROST_LOCAL_AUTH_TOKEN`
  - `Claude Code -> Bifrost` 的本地 token
- `BIFROST_UPSTREAM_BASE_URL`
  - `Bifrost -> agent-runner api-proxy` 的地址
  - 默认应为 `http://host.docker.internal:9000`
- `BIFROST_UPSTREAM_AUTH_TOKEN`
  - `Bifrost -> agent-runner api-proxy` 使用的 token
  - 本次允许与 `BIFROST_LOCAL_AUTH_TOKEN` 相同
- `BIFROST_LOCKED_MODEL`
  - DogBot 当前唯一允许的模型

实现上，`agent-runner` 仍然负责把这些 env 注入到容器里，因为容器创建仍然由 `agent-runner` 的 `ContainerSpec` 管理。

### 配置优先级

运行期模型配置以 `BIFROST_LOCKED_MODEL` 为准。

以下配置都不再被视为正式模型控制面：

- `API_PROXY_UPSTREAM_MODEL`
- Claude CLI 内部的交互式 `/model`
- 容器内临时设置的 `ANTHROPIC_DEFAULT_*_MODEL`

这些项即使存在，也不能成为 DogBot 的正式运维入口。

## `agent-runner` 需要做什么

`agent-runner` 仍然保留宿主机 proxy，但边界变得更简单：

- 保留
  - 上游 base URL
  - 上游 token
  - 上游 auth header / scheme
  - 通用请求转发
- 删除
  - `API_PROXY_UPSTREAM_MODEL`
  - `ProviderConfig.model`
  - `/v1/messages` 的 `model` 覆盖逻辑

换句话说，宿主机 proxy 应当把“这次请求要打哪个 model”视为 Bifrost 已经决定好的请求内容，而不是自己二次改写。

## 为什么删除 `API_PROXY_UPSTREAM_MODEL`

删除它有三个原因：

1. 它把模型语义放在了错误的层级  
   `agent-runner` 只应该处理鉴权、转发和运行控制，不应该承担 provider/model 语义。

2. 它只对一部分协议路径稳定  
   当前代码只会在 `/v1/messages` 上改写 `model`，这种方式对未来多 provider、多协议的扩展不够稳。

3. 它和容器内模型控制冲突  
   如果 Bifrost 已经决定模型，而宿主机 proxy 又二次覆盖，请求真实落点会变得不可预测。

因此本次设计要求彻底删除，而不是“继续保留但不推荐使用”。

## 容器内改环境变量是否还能改模型

从当前项目的目标出发，这个问题要分成两层说明。

### 正常运行路径下

如果 `Claude Code` 只走默认运行路径：

- 它看到的是 `127.0.0.1:<BIFROST_PORT>/anthropic`
- 它即使修改自己的环境变量，也不会回溯修改已启动的 `Bifrost` 进程配置
- 它即使切换 Claude 自己的 model env，也不会改变 `BIFROST_LOCKED_MODEL`

因此对“正常运行路径”来说，模型仍然是锁住的。

### 对抗式绕过下

如果把容器内进程视为主动攻击者，那么它理论上仍然可能：

- 猜到或读到宿主机 proxy 端口
- 直接构造 HTTP 请求绕过 `Bifrost`
- 在知道 token 的前提下直连宿主机 proxy

本次设计接受这个限制，并明确写成已知问题，而不是假装已经实现了硬隔离。

也就是说，本次交付的是：

- 运行路径锁定
- 运维入口收敛
- 误操作防护

而不是：

- 对抗恶意容器内进程的强安全隔离

## 已知限制

本次方案需要在文档中明确写出以下限制：

- 它是“软隔离 + 稳定运行路径”，不是严格安全边界
- 如果未来安全要求上升，必须补 token split 或 sidecar
- `Bifrost` 选用的具体 provider/model 仍然要满足 Claude Code 所需的 tool use 能力，否则虽然链路可通，实际执行会失败

## 代码与文档改动范围

### 需要修改

- `docker/claude-runner/Dockerfile`
  - 安装 `Bifrost`
- `docker/claude-runner/entrypoint.sh`
  - 启动并守护 `Bifrost`
- `agent-runner/src/config.rs`
  - 新增 `BIFROST_*` 配置读取
- `agent-runner/src/docker_client.rs`
  - 容器 env 改为注入 `Claude -> Bifrost` 和 `Bifrost -> host proxy` 所需参数
- `agent-runner/src/api_proxy_config.rs`
  - 删除 `model` 字段
- `agent-runner/src/api_proxy.rs`
  - 删除请求体 `model` 改写逻辑
- `agent-runner/tests/api_proxy_tests.rs`
  - 删除或重写覆盖模型的相关测试
- `deploy/dogbot.env.example`
  - 新增 `BIFROST_*`
  - 删除 `API_PROXY_UPSTREAM_MODEL`
- `deploy/README.md`
  - 改写模型配置说明
- `README.md`
  - 更新 TODO

### 不需要修改

- `qq_adapter`
- `wechatpadpro_adapter`
- control-plane 语义
- inbound / history / memory 相关逻辑

## 验收标准

实现后至少要满足以下验收条件：

1. `Claude Code` 默认请求路径为：
   `Claude Code -> 127.0.0.1:<BIFROST_PORT>/anthropic -> host.docker.internal:9000`
2. `agent-runner` 代码中不再存在 `API_PROXY_UPSTREAM_MODEL`
3. `agent-runner` 代码中不再存在请求体 `model` 覆盖逻辑
4. 修改 `BIFROST_LOCKED_MODEL` 并重启后，请求落到新模型
5. 不重启时，容器内临时改 Claude 侧环境变量不会改变已运行的 `Bifrost` 目标模型
6. `Bifrost` 异常退出时，容器会跟随退出或至少让运行链路明确失败，而不是静默保活

## 迁移方式

迁移按一次性切换处理：

1. 在 `claude-runner` 镜像中加入 `Bifrost`
2. 调整容器启动脚本，使 `Bifrost` 成为容器可用性的前提
3. 在 `agent-runner` 中新增 `BIFROST_*` 容器 env 注入
4. 删除 `API_PROXY_UPSTREAM_MODEL`
5. 更新部署文档和示例配置
6. 用单个固定模型完成 smoke test

切换完成后，DogBot 的模型选择入口只保留：

- `deploy/dogbot.env`
  - `BIFROST_LOCKED_MODEL`

不再鼓励任何“进入容器手动改 Claude 默认模型”的运维方式。
