# DogBot 易用性整理与去除 AstrBot 依赖设计

## 目标

本次整理的目标不是单独优化文案，而是把项目的部署体验和运行结构一起收敛到更简单的形态：

```text
QQ -> NapCat -> qq-adapter -> agent-runner -> claude-runner
微信 -> WeChatPadPro -> wechatpadpro-adapter -> agent-runner -> claude-runner
```

也就是说：

- 去掉 `AstrBot`
- QQ 和微信都通过宿主机上的薄 adapter 接入
- 用户只需要维护一个配置文件
- 用户只需要两个脚本完成部署和停止

## 核心诉求

这次整理只围绕 4 个易用性目标展开：

1. 易于配置  
   用户只需要关心 `deploy/dogbot.env`
2. 易于部署  
   用户执行 `./scripts/deploy_stack.sh` 就能完成部署、登录准备和接入配置
3. 易于停止  
   用户执行 `./scripts/stop_stack.sh` 就能完整停止所有服务
4. 易于 debug  
   脚本要有足够的注释、明确的报错和下一步提示

## 为什么要去掉 AstrBot

当前最大的阻碍不是“功能不够”，而是部署路径里仍然有一段需要人工进入 WebUI 完成配置。这和一键部署目标冲突。

`AstrBot` 的问题在于：

- 需要额外的 WebUI 操作
- QQ 接入逻辑被多一层平台框架包裹
- 命令预处理、消息格式、平台行为会被框架改写
- 调试路径变长，问题很难快速归因

所以本次设计直接按“没有 AstrBot”的最终形态规划，而不是先做一个临时过渡方案。

## 设计结果

本次整理后的系统需要满足 5 个结果。

### 1. `deploy/dogbot.env` 是唯一用户配置入口

用户不再需要：

- 修改 `compose/*.yml`
- 手动编辑平台配置文件
- 进入 WebUI 填平台参数

所有用户可配置项统一放在 `deploy/dogbot.env` 中，包括：

- 宿主机目录
- 端口
- Claude 容器资源限制
- 上游模型源地址和 token
- QQ 开关
- 微信开关
- 平台登录和适配器参数
- 触发规则

`compose/` 目录仍然保留，但默认视为高级层，不再是日常部署入口。

### 2. `./scripts/deploy_stack.sh` 是唯一部署入口

部署脚本需要承担完整的启动职责，而不是只做 `docker compose up`。

脚本应当负责：

- 检查依赖
- 读取并校验 `deploy/dogbot.env`
- 创建宿主机目录
- 启动 `agent-runner`
- 启动 `claude-runner`
- 启动 `NapCat`
- 启动 `WeChatPadPro`
- 启动 `qq-adapter`
- 启动 `wechatpadpro-adapter`
- 自动完成平台接入所需的配置写入
- 拉起二维码登录流程
- 输出清晰的下一步操作

### 3. `./scripts/stop_stack.sh` 是唯一停止入口

停止脚本需要负责完整清理当前栈，而不是只停容器。

脚本应当负责：

- 停止所有 compose 容器
- 停止宿主机上的 `agent-runner`
- 停止宿主机上的 `qq-adapter`
- 停止宿主机上的 `wechatpadpro-adapter`
- 删除 PID 文件
- 清理网络策略
- 输出停止结果

### 4. `compose/` 目录只作为高级配置层

README 中需要明确说明：

- 一般情况下不需要修改 `compose/`
- 如果确实需要自定义容器层行为，请看 `compose/README.md`

同时需要新增 `compose/README.md`，说明：

- 每个 compose 文件负责什么
- 哪些配置正常不应该改
- 哪些场景下才需要改
- 改动后会影响什么

### 5. 架构收敛为“两个平台、两个 adapter、一个 runner”

最终架构：

```text
QQ -> NapCat -> qq-adapter -> agent-runner
微信 -> WeChatPadPro -> wechatpadpro-adapter -> agent-runner
```

约束：

- 平台层只做接入、消息抽取和回发
- `agent-runner` 继续负责：
  - 执行
  - 超时
  - 队列
  - 并发
  - 限流
  - 会话
  - 上游代理
- `claude-runner` 继续只负责 Docker 内 CLI Agent 运行

## 交互式部署流程

部署脚本默认要支持平台选择和二维码登录，而不是把二维码只写到文件里。

### 参数模式

如果用户为 `deploy_stack.sh` 显式传入平台参数，则按参数执行，不再交互询问。

建议支持：

- `--qq`
- `--wechat`
- `--qq --wechat`
- `--no-qq`
- `--no-wechat`

这些参数只决定本次部署要不要启用对应平台，不改变 `deploy/dogbot.env` 本身。

### 默认交互模式

如果脚本没有任何平台参数，则进入交互模式：

1. 询问是否启用 QQ
2. 询问是否启用微信

只有在用户选择启用的平台上，脚本才继续：

- 启动对应接入层
- 生成二维码
- 打印二维码
- 等待登录

### 二维码输出方式

对 QQ 和微信都采用同样策略：

1. 终端直接打印二维码
2. 同时保存二维码图片文件
3. 同时打印原始登录链接

优先使用：

- `qrencode -t ANSIUTF8`

如果宿主机没有 `qrencode`：

- 脚本打印图片路径
- 脚本打印登录链接
- 仍然允许用户继续完成登录

### 未选择任何平台时的处理

如果最终没有任何一个平台被选中，则脚本必须：

1. 输出“未选择任何平台”
2. 停掉本次已启动的服务
3. 清理 PID 文件和临时状态
4. 退出

不能继续部署一个没有入口的平台空壳。

## 脚本体验要求

### 配置校验

`deploy_stack.sh` 需要按平台做分层校验：

- 基础层：
  - `agent-runner`
  - `claude-runner`
  - 上游地址和 token
- QQ 层：
  - `NapCat` 目录
  - `qq-adapter` 相关项
- 微信层：
  - `WeChatPadPro`
  - `wechatpadpro-adapter`
  - MySQL / Redis

脚本只校验当前启用的平台需要的字段，避免用户因为未启用的平台配置缺失而无法部署。

### 错误提示

脚本报错必须尽量给出下一步行动，而不是只抛底层错误。

例如：

- Docker Compose 不可用
  - 明确提示安装 Compose v2
- 镜像拉取失败
  - 明确提示检查 Docker Hub 出口或代理
- 未填写上游 token
  - 明确提示去 `deploy/dogbot.env` 补哪一项
- 平台未登录
  - 明确提示扫描哪个二维码、在哪个文件里能看到图片和链接

### 调试友好性

脚本需要：

- 注释清楚
- 输出关键步骤
- 出错时保留必要上下文
- 不把核心失败信息吞掉

## 配置收敛原则

`deploy/dogbot.env` 仍然保留两类内容：

1. 用户需要改的内容
   - 目录
   - 端口
   - 上游地址
   - token
   - 平台选择
   - 平台登录和 adapter 参数

2. 用户通常不需要改的高级内容
   - 默认资源限制
   - 默认日志路径
   - 平台高级调试开关

但无论哪一类，都必须仍然集中在同一个文件中，而不是散到多处配置。

## 对 `compose/` 的要求

`compose/` 目录需要保留，但定位改为：

- 仓库内部部署模板
- 高级用户自定义入口

而不是普通用户日常配置入口。

需要新增：

- `compose/README.md`

内容至少包括：

- `docker-compose.yml`
  - 负责 `claude-runner`
- `platform-stack.yml`
  - 负责 `NapCat`
- `wechatpadpro-stack.yml`
  - 负责 `WeChatPadPro + MySQL + Redis`
- 一般情况下为什么不需要改它们
- 在哪些场景下才建议改：
  - 自定义镜像
  - 自定义挂载
  - 自定义资源限制
  - 自定义端口映射

## 实施拆分建议

这次整理建议拆成 4 个实现块：

1. 文档与配置层收敛
   - README
   - deploy/README
   - compose/README
   - `deploy/dogbot.env.example`

2. 脚本层重整
   - `deploy_stack.sh`
   - `stop_stack.sh`
   - 平台选择
   - 二维码打印
   - 错误提示

3. QQ adapter 替换 AstrBot
   - 新建宿主机 `qq-adapter`
   - 迁移 `claude_runner_bridge` 中当前有效逻辑

4. 编排层收敛
   - `platform-stack.yml` 去掉 `AstrBot`
   - 脚本和 env 全面切到新架构

## 本次设计不包含

本次设计不处理：

- 历史消息持久化
- 记忆系统
- skill 管理
- 表情与复杂媒体统一抽象
- 多种 CLI Agent 同时接入

这些内容仍属于后续主题。
