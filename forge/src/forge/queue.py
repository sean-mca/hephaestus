"""Sequential conversion queue with per-model deduplication."""

from __future__ import annotations

import asyncio
import shutil
import tempfile
from collections import defaultdict

import structlog

from forge.config import ForgeSettings
from forge.converter import convert_model, validate_model
from forge.models import ConvertResponse
from forge.storage import upload_to_s3

logger = structlog.get_logger()


class ConversionQueue:
    """Process one conversion at a time, deduplicating concurrent requests.

    * ``asyncio.Semaphore(1)`` ensures only one conversion runs at a
      time (D-10).
    * A per-``model_id`` ``asyncio.Lock`` ensures that concurrent
      requests for the same model block and receive the cached result
      rather than triggering duplicate work (D-08).
    """

    def __init__(self) -> None:
        self._semaphore = asyncio.Semaphore(1)
        self._locks: dict[str, asyncio.Lock] = defaultdict(asyncio.Lock)
        self._results: dict[str, ConvertResponse] = {}

    async def convert(
        self, model_id: str, settings: ForgeSettings
    ) -> ConvertResponse:
        """Convert *model_id* to ONNX, returning cached results when available."""
        lock = self._locks[model_id]
        async with lock:
            # D-08: Return cached result if already converted.
            if model_id in self._results:
                logger.info("conversion_cache_hit", model_id=model_id)
                return self._results[model_id]

            # D-10: Only one conversion at a time.
            async with self._semaphore:
                output_dir = tempfile.mkdtemp(prefix="forge-")
                try:
                    # Let the conversion run to completion.
                    # Timeout enforcement moves to the HTTP client (Hephaestus
                    # HttpForgeClient already has FORGE_TIMEOUT_SECS).
                    # asyncio.wait_for cannot actually cancel thread pool work
                    # started by asyncio.to_thread -- the thread keeps running
                    # after cancellation, violating the D-10 one-at-a-time
                    # guarantee and risking OOM from concurrent conversions.
                    result = await self._do_convert(model_id, output_dir, settings)
                    self._results[model_id] = result
                    return result
                except Exception:
                    # Clean up temp dir on any failure.
                    shutil.rmtree(output_dir, ignore_errors=True)
                    raise

    async def _do_convert(
        self,
        model_id: str,
        output_dir: str,
        settings: ForgeSettings,
    ) -> ConvertResponse:
        """Run conversion, validation, and S3 upload in a thread pool."""
        logger.info("conversion_start", model_id=model_id)

        metadata = await asyncio.to_thread(convert_model, model_id, output_dir)
        logger.info("conversion_export_done", model_id=model_id)

        await asyncio.to_thread(validate_model, output_dir)
        logger.info("conversion_validated", model_id=model_id)

        s3_paths = await asyncio.to_thread(
            upload_to_s3,
            output_dir,
            settings.s3_bucket,
            settings.s3_prefix,
            model_id,
        )
        logger.info(
            "conversion_uploaded",
            model_id=model_id,
            s3_paths=s3_paths,
        )

        # Clean up temp dir after successful upload.
        shutil.rmtree(output_dir, ignore_errors=True)

        return ConvertResponse(s3_paths=s3_paths, metadata=metadata)
