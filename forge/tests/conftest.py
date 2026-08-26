"""Shared test fixtures for the Forge test suite."""

from __future__ import annotations

import json
import os
import tempfile
from typing import Iterator

import opendal
import pytest

from forge.config import ForgeSettings

TEST_PREFIX = "models"


@pytest.fixture()
def test_settings() -> ForgeSettings:
    """Return a ForgeSettings instance configured for testing."""
    return ForgeSettings(
        storage_type="memory",
        storage_bucket="",
        storage_prefix=TEST_PREFIX,
        storage_root="",
        storage_region="",
        conversion_timeout_secs=60,
        log_level="debug",
    )


@pytest.fixture()
def memory_operator() -> opendal.Operator:
    """Return an OpenDAL memory-backed Operator for testing."""
    return opendal.Operator("memory")


@pytest.fixture()
def tmp_output_dir() -> Iterator[str]:
    """Provide a temporary directory for model output and clean up after."""
    d = tempfile.mkdtemp(prefix="forge-test-")
    yield d
    import shutil

    shutil.rmtree(d, ignore_errors=True)


@pytest.fixture()
def populated_output_dir(tmp_output_dir: str) -> str:
    """Create a minimal set of model artifacts in the temp directory.

    This is a lightweight fixture for storage tests -- it writes
    small placeholder files, not real ONNX models.
    """
    for name, content in [
        ("model.onnx", b"fake-onnx-bytes"),
        ("tokenizer.json", json.dumps({"type": "test"}).encode()),
        ("config.json", json.dumps({"model_type": "test"}).encode()),
    ]:
        with open(os.path.join(tmp_output_dir, name), "wb") as f:
            f.write(content)
    return tmp_output_dir
