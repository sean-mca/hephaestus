"""Shared test fixtures for the Forge test suite."""

from __future__ import annotations

import json
import os
import tempfile
from typing import Iterator

import boto3
import pytest
from moto import mock_aws

from forge.config import ForgeSettings

TEST_BUCKET = "test-bucket"
TEST_PREFIX = "models"


@pytest.fixture()
def test_settings() -> ForgeSettings:
    """Return a ForgeSettings instance configured for testing."""
    return ForgeSettings(
        s3_bucket=TEST_BUCKET,
        s3_prefix=TEST_PREFIX,
        conversion_timeout_secs=60,
        log_level="debug",
    )


@pytest.fixture()
def tmp_output_dir() -> Iterator[str]:
    """Provide a temporary directory for model output and clean up after."""
    d = tempfile.mkdtemp(prefix="forge-test-")
    yield d
    import shutil

    shutil.rmtree(d, ignore_errors=True)


@pytest.fixture()
def s3_mock():
    """Start a moto S3 mock and create the test bucket."""
    with mock_aws():
        s3 = boto3.client("s3", region_name="us-east-1")
        s3.create_bucket(Bucket=TEST_BUCKET)
        yield s3


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
