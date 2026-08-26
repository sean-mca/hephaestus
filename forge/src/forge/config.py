"""Forge service configuration via environment variables."""

from __future__ import annotations

from typing import Optional

from pydantic_settings import BaseSettings


class ForgeSettings(BaseSettings):
    """Configuration for the Forge conversion service.

    Each field maps to an environment variable of the same name
    (case-insensitive). For example, ``S3_BUCKET`` sets ``s3_bucket``.
    """

    s3_bucket: str = ""
    s3_prefix: str = ""
    conversion_timeout_secs: int = 540
    hf_token: Optional[str] = None
    log_level: str = "info"
    host: str = "0.0.0.0"
    port: int = 8080
