from .aiohttp_config import AiohttpSettings

AIOHTTP_SETTINGS = AiohttpSettings()


APPS = {
    "aiohttp": "fectl.apps.aiohttp.AiohttpRunner",
}
