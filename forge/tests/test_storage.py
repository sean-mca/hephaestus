"""Tests for forge.storage -- S3 upload logic."""

from __future__ import annotations

import os

from moto import mock_aws

from forge.storage import upload_to_s3
from tests.conftest import TEST_BUCKET, TEST_PREFIX


@mock_aws
def test_upload_to_s3_with_prefix(populated_output_dir: str, s3_mock) -> None:
    """Uploaded keys follow the {prefix}/{model_id}/{filename} layout."""
    model_id = "org/my-model"
    keys = upload_to_s3(populated_output_dir, TEST_BUCKET, TEST_PREFIX, model_id)

    assert len(keys) == 3
    for key in keys:
        assert key.startswith(f"{TEST_PREFIX}/{model_id}/")
    assert f"{TEST_PREFIX}/{model_id}/model.onnx" in keys
    assert f"{TEST_PREFIX}/{model_id}/tokenizer.json" in keys
    assert f"{TEST_PREFIX}/{model_id}/config.json" in keys


@mock_aws
def test_upload_to_s3_without_prefix(populated_output_dir: str, s3_mock) -> None:
    """When prefix is empty, keys are {model_id}/{filename}."""
    model_id = "org/my-model"
    keys = upload_to_s3(populated_output_dir, TEST_BUCKET, "", model_id)

    assert len(keys) == 3
    for key in keys:
        assert key.startswith(f"{model_id}/")
        assert not key.startswith("/")


@mock_aws
def test_uploaded_files_are_retrievable(populated_output_dir: str, s3_mock) -> None:
    """Files uploaded by upload_to_s3 can be retrieved from S3."""
    model_id = "org/my-model"
    keys = upload_to_s3(populated_output_dir, TEST_BUCKET, TEST_PREFIX, model_id)

    for key in keys:
        obj = s3_mock.get_object(Bucket=TEST_BUCKET, Key=key)
        body = obj["Body"].read()
        assert len(body) > 0


def test_upload_includes_subdirectory_files(populated_output_dir: str, s3_mock) -> None:
    """Files in subdirectories are included in the upload (recursive walk)."""
    subdir = os.path.join(populated_output_dir, "onnx")
    os.makedirs(subdir)
    with open(os.path.join(subdir, "model.onnx"), "wb") as f:
        f.write(b"onnx-subdir-model")
    model_id = "org/my-model"
    keys = upload_to_s3(populated_output_dir, TEST_BUCKET, TEST_PREFIX, model_id)

    # 3 original files + 1 file in onnx/ subdirectory.
    assert len(keys) == 4
    assert f"{TEST_PREFIX}/{model_id}/onnx/model.onnx" in keys
