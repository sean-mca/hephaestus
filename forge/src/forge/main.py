"""Forge application factory and entry point."""

from __future__ import annotations

from contextlib import asynccontextmanager
from typing import AsyncIterator

import structlog
import uvicorn
from fastapi import FastAPI

from forge.api import router
from forge.config import ForgeSettings
from forge.queue import ConversionQueue


@asynccontextmanager
async def lifespan(app: FastAPI) -> AsyncIterator[None]:
    """Application lifespan: configure logging, settings, and queue."""
    settings = ForgeSettings()

    structlog.configure(
        processors=[
            structlog.contextvars.merge_contextvars,
            structlog.processors.add_log_level,
            structlog.processors.TimeStamper(fmt="iso"),
            structlog.processors.JSONRenderer(),
        ],
        wrapper_class=structlog.make_filtering_bound_logger(
            structlog.get_level_from_name(settings.log_level),
        ),
    )

    logger = structlog.get_logger()
    logger.info("forge_starting", host=settings.host, port=settings.port)

    app.state.settings = settings
    app.state.queue = ConversionQueue()

    yield

    logger.info("forge_shutting_down")


def create_app() -> FastAPI:
    """Build and return the Forge FastAPI application."""
    app = FastAPI(
        title="Forge",
        description="ONNX model conversion service for Hephaestus",
        version="0.1.0",
        lifespan=lifespan,
    )
    app.include_router(router)

    @app.get("/health")
    async def health() -> dict[str, str]:
        return {"status": "ok"}

    return app


if __name__ == "__main__":
    settings = ForgeSettings()
    uvicorn.run(
        "forge.main:create_app",
        factory=True,
        host=settings.host,
        port=settings.port,
    )
