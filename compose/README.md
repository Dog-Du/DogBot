# compose 目录说明

一般情况下不需要直接修改本目录。

本目录中的文件用于定义容器层运行方式：

- `docker-compose.yml`
  - 定义 `claude-runner`
- `platform-stack.yml`
  - 定义 `napcat` 和当前遗留的 `astrbot`
- `wechatpadpro-stack.yml`
  - 定义 `wechatpadpro`、`mysql`、`redis`

只有在以下场景才建议直接修改：

- 自定义镜像名
- 自定义端口映射
- 自定义 volume 挂载
- 自定义资源限制

普通用户应优先通过 `deploy/dogbot.env` 调整配置。
