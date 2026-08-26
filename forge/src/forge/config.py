"""Forge service configuration via environment variables."""

from __future__ import annotations

from pydantic_settings import BaseSettings


class ForgeSettings(BaseSettings):
    """Configuration for the Forge conversion service.

    Each field maps to an environment variable of the same name
    (case-insensitive). For example, ``STORAGE_TYPE`` sets
    ``storage_type``, ``STORAGE_BUCKET`` sets ``storage_bucket``.

    HuggingFace authentication is handled by the ``HF_TOKEN``
    environment variable, which the ``transformers`` and ``optimum``
    libraries read directly. No explicit config field is needed.
    """

    storage_type: str = "s3"
    storage_bucket: str = ""
    storage_prefix: str = ""
    storage_root: str = ""
    storage_region: str = ""
    conversion_timeout_secs: int = 540
    log_level: str = "info"
    host: str = "0.0.0.0"
    port: int = 8080
