# Platform Login Blocking Design

## Goal

让 `deploy_stack.sh` 在 QQ 和微信登录阶段表现一致：

- 登录二维码必须刷新覆盖旧文件
- 脚本必须阻塞等待登录成功
- 超时时间固定为 100 秒
- 登录未完成时不能继续推进后续部署步骤

同时清理已经退出运行链路的 `astrbot/` 目录及其当前活跃引用。

## Current Problems

### QQ

当前 [`scripts/prepare_napcat_login.sh`](/home/dogdu/workspace/dogbot/scripts/prepare_napcat_login.sh) 只会尝试把容器内现有的 `/app/napcat/cache/qrcode.png` 拷贝到宿主机。如果 NapCat 还没生成新的二维码，宿主机上的旧 [`napcat-login-qr.png`](/home/dogdu/workspace/dogbot/agent-state/napcat-login/napcat-login-qr.png) 会被保留，操作员看到的就是过期图片。

脚本也不会等待 NapCat 登录成功，只是打印一次二维码路径和登录链接，然后立刻返回。

### WeChatPadPro

当前 [`scripts/prepare_wechatpadpro_login.sh`](/home/dogdu/workspace/dogbot/scripts/prepare_wechatpadpro_login.sh) 只负责：

- 确保 `WECHATPADPRO_ACCOUNT_KEY`
- 拉取一次二维码
- 把二维码图片和元信息写到宿主机

它不会继续轮询扫码进度和最终在线状态，因此 `deploy_stack.sh` 无法知道微信是否真的登录成功。

### Deploy Flow

[`scripts/deploy_stack.sh`](/home/dogdu/workspace/dogbot/scripts/deploy_stack.sh) 当前的登录阶段只是串行调用“准备二维码”脚本，不符合现有设计文档中“打印二维码并等待登录”的预期。

## Constraints

- 超时时间固定为 100 秒
- 失败必须非零退出
- 只有探测到登录成功之后才能继续推进后续步骤
- QQ 和微信都必须在宿主机上保留二维码图片和元信息文件
- 历史设计/计划文档可以保留；运行链路和当前说明文档必须去掉 `astrbot` 作为现役组件的表述

## External Signals

### NapCat

NapCat 官方接口文档列出了 `POST /get_login_info` 和 `POST /get_status`，可用于判断当前 QQ 账号是否已经登录并可提供 OneBot 服务。设计上优先使用 `get_login_info` 作为成功信号，要求返回成功并能解析出登录号信息；必要时可用 `get_status` 作为辅助判断。

来源：

- NapCat 官方接口文档 `账号相关`

### WeChatPadPro

WeChatPadPro 官方项目文档列出了：

- `GET /api/login/CheckLoginStatus`
- `GET /api/login/GetLoginStatus`

当前仓库里使用的部署镜像实际暴露的路径风格是 `/login/...`，例如现有脚本已经调用 `/login/GetLoginQrCodePadX`。设计上采用当前镜像的路径风格，优先轮询 `/login/GetLoginStatus` 判定最终在线状态，并在需要时使用 `/login/CheckLoginStatus` 观测扫码进度或二维码过期状态。

来源：

- WeChatPadPro 官方 GitHub README 的登录接口章节

## Design

### 1. Unified Blocking Login Phase

`deploy_stack.sh` 的平台启动流程保持现有顺序，但登录步骤改为显式阻塞阶段：

1. 启动核心服务
2. 启动平台容器与 adapter
3. 生成并输出二维码
4. 阻塞等待登录成功
5. 登录成功后才继续后续配置

如果 100 秒内仍未登录成功：

- 当前脚本直接非零退出
- 已启动的容器和 adapter 保持运行，便于操作员继续排查或立即重试
- 终端明确打印超时说明和二维码文件路径

不在超时分支中自动停栈，避免把外部扫码场景和排障上下文一起销毁。

### 2. Common Login Helpers

在 [`scripts/lib/common.sh`](/home/dogdu/workspace/dogbot/scripts/lib/common.sh) 中补充统一辅助函数，避免 QQ 和微信各自实现一套轮询逻辑：

- 超时轮询封装
- 轮询间隔控制
- 文件变更检测
- 二维码终端输出复用
- 超时消息格式统一

该层只提供“等待某个条件成立”的通用能力，不直接理解 NapCat 或 WeChatPadPro 业务。

### 3. QQ Login Flow

[`scripts/prepare_napcat_login.sh`](/home/dogdu/workspace/dogbot/scripts/prepare_napcat_login.sh) 改成一个完整的“准备 + 等待”脚本：

1. 删除宿主机上旧的二维码图片和元信息文件
2. 轮询 NapCat 容器中的二维码文件和最新登录链接
3. 一旦拿到新的二维码或新的登录链接，就覆盖本地输出文件
4. 同时在终端打印二维码链接；如果装有 `qrencode`，继续打印终端二维码
5. 轮询 NapCat HTTP API 的 `get_login_info`
6. 一旦返回有效登录号，则认为 QQ 登录成功并返回 0
7. 超过 100 秒仍未成功则返回非零

关键要求：

- 本地 [`napcat-login-qr.png`](/home/dogdu/workspace/dogbot/agent-state/napcat-login/napcat-login-qr.png) 必须由“本轮登录流程拿到的最新二维码”覆盖生成，不能保留旧图片
- 元信息文件中增加本轮更新时间和当前登录链接，方便判断是不是新码
- 如果始终没拿到新二维码，也必须以失败退出，而不是默默保留旧文件

### 4. WeChatPadPro Login Flow

[`scripts/prepare_wechatpadpro_login.sh`](/home/dogdu/workspace/dogbot/scripts/prepare_wechatpadpro_login.sh) 改成完整的“拉码 + 状态轮询 + 成功判定”流程：

1. 确保 WeChatPadPro API 已 ready
2. 确保 `WECHATPADPRO_ACCOUNT_KEY`
3. 请求登录二维码
4. 把二维码图片和元信息覆盖写入宿主机
5. 在终端打印二维码链接；如果本机装有 `qrencode`，继续打印终端二维码
6. 轮询 `/login/GetLoginStatus`
7. 如状态接口表明二维码失效或需要重新拉码，则再次请求二维码并覆盖本地文件
8. 一旦状态接口明确显示当前 key 已在线或已绑定微信号，则认为登录成功并返回 0
9. 超过 100 秒仍未成功则返回非零

当前范围只覆盖二维码扫码登录。若接口进入验证码补交流程，脚本需要明确打印“需要额外验证码处理”，并以失败退出，而不是伪装成正常登录完成。

### 5. Deploy Script Changes

[`scripts/deploy_stack.sh`](/home/dogdu/workspace/dogbot/scripts/deploy_stack.sh) 的平台阶段调整为：

#### QQ

1. 启动 `qq-adapter`
2. 启动 `napcat`
3. 配置反向 WebSocket
4. 调用 `prepare_napcat_login.sh`
5. 只有返回 0 才继续后续平台步骤

#### 微信

1. 启动 `wechatpadpro` / `mysql` / `redis`
2. 启动 `wechatpadpro-adapter`
3. 调用 `prepare_wechatpadpro_login.sh`
4. 只有返回 0 才继续执行 webhook 自动配置

这使得“是否已登录”成为部署成功的组成部分，而不是部署后仍需人工猜测的外部条件。

### 6. `astrbot/` Cleanup

`astrbot/` 已经不在当前运行链路中，应从仓库中移除：

- 删除 [`astrbot/`](/home/dogdu/workspace/dogbot/astrbot)
- 删除当前测试入口对它的依赖
- 更新当前有效文档中的组件说明和阅读路径

历史设计/计划文档保留，不追溯性篡改旧记录；但像 [`AGENTS.md`](/home/dogdu/workspace/dogbot/AGENTS.md) 这类当前入口文档必须同步到新链路。

## Error Handling

### QQ

- 100 秒内未拿到新二维码：失败退出
- 100 秒内拿到二维码但未登录成功：失败退出
- `get_login_info` 返回错误：继续轮询直到超时

### 微信

- 100 秒内未拿到二维码：失败退出
- 状态接口显示需要验证码：失败退出并打印提示
- 100 秒内未进入在线状态：失败退出

### Deploy

- 任一启用平台登录失败：`deploy_stack.sh` 立即非零退出
- 未执行到的平台后续步骤不再继续

## Testing

测试覆盖分三层：

### Shell Regression Tests

新增脚本测试，验证：

- QQ 旧二维码文件会先被删除，再被新二维码覆盖
- QQ 登录成功前 `prepare_napcat_login.sh` 不会提前退出
- 微信二维码会被写出到固定文件名
- 微信未在线时脚本会持续轮询
- 登录成功后脚本立即返回 0
- 超过 100 秒时脚本返回非零

### Structure Checks

[`scripts/check_structure.sh`](/home/dogdu/workspace/dogbot/scripts/check_structure.sh) 继续保证关键脚本存在且可执行，并加入新测试文件执行。

### End-to-End Verification

手工验证要求：

1. 退出 QQ 和微信登录态
2. 运行 `sudo ./deploy_stack.sh`
3. 确认终端输出新的 QQ / 微信二维码
4. 在 100 秒内扫码
5. 确认脚本只在检测到登录成功后继续完成部署

## Non-Goals

- 不重构 `qq_adapter` 或 `wechatpadpro_adapter` 的消息处理逻辑
- 不引入新的长期守护进程处理登录状态
- 不改动历史设计文档中的历史叙述
