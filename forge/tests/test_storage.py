"""Tests for forge.storage -- OpenDAL-based upload logic."""

from __future__ import annotations

import os

import opendal

from forge.storage import upload_to_storage


def test_upload_to_storage_writes_files(
    populated_output_dir: str, memory_operator: opendal.Operator
) -> None:
    """Uploaded paths follow the {model_id}/{filename} layout."""
    model_id = "org/my-model"
    paths = upload_to_storage(memory_operator, model_id, populated_output_dir)

    assert len(paths) == 3
    for path in paths:
        assert path.startswith(f"{model_id}/")
    assert f"{model_id}/model.onnx" in paths
    assert f"{model_id}/tokenizer.json" in paths
    assert f"{model_id}/config.json" in paths

    # Verify files are readable from the operator.
    for path in paths:
        data = memory_operator.read(path)
        assert len(data) > 0


def test_upload_paths_contain_model_id(
    populated_output_dir: str, memory_operator: opendal.Operator
) -> None:
    """All uploaded paths start with model_id and never with a leading slash."""
    model_id = "org/my-model"
    paths = upload_to_storage(memory_operator, model_id, populated_output_dir)

    assert len(paths) == 3
    for path in paths:
        assert path.startswith(f"{model_id}/")
        assert not path.startswith("/")


def test_uploaded_files_are_readable(
    populated_output_dir: str, memory_operator: opendal.Operator
) -> None:
    """Files uploaded via upload_to_storage can be read back."""
    model_id = "org/my-model"
    paths = upload_to_storage(memory_operator, model_id, populated_output_dir)

    for path in paths:
        data = memory_operator.read(path)
        assert len(data) > 0


def test_upload_includes_subdirectories(
    populated_output_dir: str, memory_operator: opendal.Operator
) -> None:
    """Files in subdirectories are included in the upload (recursive walk)."""
    subdir = os.path.join(populated_output_dir, "onnx")
    os.makedirs(subdir)
    with open(os.path.join(subdir, "model.onnx"), "wb") as f:
        f.write(b"onnx-subdir-model")

    model_id = "org/my-model"
    paths = upload_to_storage(memory_operator, model_id, populated_output_dir)

    # 3 original files + 1 file in onnx/ subdirectory.
    assert len(paths) == 4
    assert f"{model_id}/onnx/model.onnx" in paths
