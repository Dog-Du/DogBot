from wechatpadpro_adapter.config import Settings


def test_settings_use_expected_defaults(monkeypatch):
    monkeypatch.delenv("WECHATPADPRO_ADAPTER_BIND_ADDR", raising=False)
    monkeypatch.delenv("WECHATPADPRO_BASE_URL", raising=False)
    monkeypatch.delenv("AGENT_RUNNER_BASE_URL", raising=False)
    monkeypatch.delenv("WECHATPADPRO_ACCOUNT_KEY", raising=False)
    monkeypatch.delenv("WECHATPADPRO_ADAPTER_SHARED_TOKEN", raising=False)
    monkeypatch.delenv("WECHATPADPRO_WEBHOOK_SECRET", raising=False)
    monkeypatch.delenv("WECHATPADPRO_ADAPTER_WEBHOOK_URL", raising=False)
    monkeypatch.delenv("WECHATPADPRO_AUTO_CONFIGURE_WEBHOOK", raising=False)
    monkeypatch.delenv("WECHATPADPRO_DEFAULT_CWD", raising=False)
    monkeypatch.delenv("WECHATPADPRO_DEFAULT_TIMEOUT_SECS", raising=False)

    settings = Settings.from_env()

    assert settings.bind_addr == "0.0.0.0:18999"
    assert settings.wechatpadpro_base_url == "http://127.0.0.1:38849"
    assert settings.agent_runner_base_url == "http://127.0.0.1:11451"
    assert settings.account_key is None
    assert settings.shared_token is None
    assert settings.webhook_secret is None
    assert settings.adapter_webhook_url is None
    assert settings.auto_configure_webhook is False
    assert settings.default_cwd == "/workspace"
    assert settings.default_timeout_secs == 120
