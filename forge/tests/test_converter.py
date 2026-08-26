"""Tests for forge.converter -- conversion and validation logic."""

from __future__ import annotations

import json
import os

import numpy as np
import onnx
import pytest
from onnx import TensorProto, helper

from forge.converter import ConversionError, validate_model


def _make_minimal_onnx(output_dir: str) -> str:
    """Create a minimal valid ONNX model in *output_dir*.

    The model computes ``output = input + input`` with a single Add node.
    """
    X = helper.make_tensor_value_info("input", TensorProto.FLOAT, [1, 4])
    Y = helper.make_tensor_value_info("output", TensorProto.FLOAT, [1, 4])

    add_node = helper.make_node("Add", inputs=["input", "input"], outputs=["output"])

    graph = helper.make_graph([add_node], "test_graph", [X], [Y])
    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 13)])
    model.ir_version = 7

    path = os.path.join(output_dir, "model.onnx")
    onnx.save(model, path)
    return path


def _write_json(path: str, data: dict) -> None:
    with open(path, "w") as f:
        json.dump(data, f)


class TestValidateModelSuccess:
    """validate_model succeeds with a valid minimal ONNX model + artifacts."""

    def test_valid_model_passes(self, tmp_output_dir: str) -> None:
        _make_minimal_onnx(tmp_output_dir)
        _write_json(
            os.path.join(tmp_output_dir, "tokenizer.json"), {"type": "test"}
        )
        _write_json(
            os.path.join(tmp_output_dir, "config.json"), {"model_type": "test"}
        )

        # Should not raise.
        validate_model(tmp_output_dir)


class TestValidateModelFailures:
    """validate_model raises ConversionError for missing or invalid artifacts."""

    def test_missing_model_onnx(self, tmp_output_dir: str) -> None:
        _write_json(
            os.path.join(tmp_output_dir, "tokenizer.json"), {"type": "test"}
        )
        _write_json(
            os.path.join(tmp_output_dir, "config.json"), {"model_type": "test"}
        )

        with pytest.raises(ConversionError, match="model.onnx not found"):
            validate_model(tmp_output_dir)

    def test_missing_tokenizer_json(self, tmp_output_dir: str) -> None:
        _make_minimal_onnx(tmp_output_dir)
        _write_json(
            os.path.join(tmp_output_dir, "config.json"), {"model_type": "test"}
        )

        with pytest.raises(ConversionError, match="tokenizer.json not found"):
            validate_model(tmp_output_dir)

    def test_missing_config_json(self, tmp_output_dir: str) -> None:
        _make_minimal_onnx(tmp_output_dir)
        _write_json(
            os.path.join(tmp_output_dir, "tokenizer.json"), {"type": "test"}
        )

        with pytest.raises(ConversionError, match="config.json not found"):
            validate_model(tmp_output_dir)

    def test_invalid_tokenizer_json(self, tmp_output_dir: str) -> None:
        _make_minimal_onnx(tmp_output_dir)
        with open(os.path.join(tmp_output_dir, "tokenizer.json"), "w") as f:
            f.write("not valid json {{{")
        _write_json(
            os.path.join(tmp_output_dir, "config.json"), {"model_type": "test"}
        )

        with pytest.raises(ConversionError, match="tokenizer.json is not valid JSON"):
            validate_model(tmp_output_dir)

    def test_invalid_config_json(self, tmp_output_dir: str) -> None:
        _make_minimal_onnx(tmp_output_dir)
        _write_json(
            os.path.join(tmp_output_dir, "tokenizer.json"), {"type": "test"}
        )
        with open(os.path.join(tmp_output_dir, "config.json"), "w") as f:
            f.write("not valid json {{{")

        with pytest.raises(ConversionError, match="config.json is not valid JSON"):
            validate_model(tmp_output_dir)
