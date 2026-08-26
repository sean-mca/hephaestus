"""ONNX model conversion and validation logic."""

from __future__ import annotations

import json
import os
import time

import numpy as np
import onnx
import onnxruntime as ort
from optimum.exporters.onnx import main_export
from transformers import AutoConfig, AutoTokenizer

from forge.models import ConversionMetadata


class ConversionError(Exception):
    """Raised when model conversion or validation fails."""


def convert_model(model_id: str, output_dir: str) -> ConversionMetadata:
    """Convert a HuggingFace model to ONNX format.

    Downloads the model from HuggingFace, exports it to ONNX via
    ``optimum``, and ensures the tokenizer is saved alongside the
    model.  Returns metadata about the conversion.
    """
    start = time.monotonic()

    main_export(
        model_name_or_path=model_id,
        output=output_dir,
        task="auto",
    )

    # Ensure tokenizer.json is present (pitfall 6: main_export may not
    # always save it in the expected format).
    tokenizer = AutoTokenizer.from_pretrained(model_id)
    tokenizer.save_pretrained(output_dir)

    elapsed = time.monotonic() - start

    config = AutoConfig.from_pretrained(model_id)

    import optimum  # noqa: E402 -- lazy import for version string

    return ConversionMetadata(
        architecture=getattr(config, "model_type", "unknown"),
        original_format="pytorch",
        conversion_duration_secs=round(elapsed, 2),
        optimum_version=optimum.__version__,
    )


def validate_model(output_dir: str) -> None:
    """Two-stage ONNX model validation.

    Stage 1: ``onnx.checker.check_model`` with a *file path string*
    (avoids loading the full protobuf into memory -- pitfall 2).

    Stage 2: Dummy inference via ``onnxruntime.InferenceSession`` to
    catch runtime errors that graph-only validation misses.

    Also verifies that ``tokenizer.json`` and ``config.json`` exist
    and are parseable (D-07).

    Raises :class:`ConversionError` on any failure.
    """

    # --- Locate model.onnx ---
    model_path = os.path.join(output_dir, "model.onnx")
    if not os.path.isfile(model_path):
        # Some exports put it under an onnx/ subdirectory.
        alt = os.path.join(output_dir, "onnx", "model.onnx")
        if os.path.isfile(alt):
            model_path = alt
        else:
            raise ConversionError(
                f"model.onnx not found in {output_dir}"
            )

    # --- Stage 1: Graph structure validation ---
    try:
        onnx.checker.check_model(model_path)
    except Exception as exc:
        raise ConversionError(
            f"onnx.checker.check_model failed: {exc}"
        ) from exc

    # --- Stage 2: Dummy inference ---
    try:
        session = ort.InferenceSession(model_path)
        dummy_inputs: dict[str, np.ndarray] = {}
        for inp in session.get_inputs():
            shape = [
                1 if d is None or isinstance(d, str) else d
                for d in inp.shape
            ]
            if inp.type == "tensor(int64)":
                dummy_inputs[inp.name] = np.ones(shape, dtype=np.int64)
            else:
                dummy_inputs[inp.name] = np.ones(shape, dtype=np.float32)
        session.run(None, dummy_inputs)
    except Exception as exc:
        raise ConversionError(
            f"dummy inference failed: {exc}"
        ) from exc

    # --- Artifact presence checks (D-07) ---
    tokenizer_path = os.path.join(output_dir, "tokenizer.json")
    if not os.path.isfile(tokenizer_path):
        raise ConversionError(
            f"tokenizer.json not found in {output_dir}"
        )
    try:
        with open(tokenizer_path) as f:
            json.load(f)
    except (json.JSONDecodeError, OSError) as exc:
        raise ConversionError(
            f"tokenizer.json is not valid JSON: {exc}"
        ) from exc

    config_path = os.path.join(output_dir, "config.json")
    if not os.path.isfile(config_path):
        raise ConversionError(
            f"config.json not found in {output_dir}"
        )
    try:
        with open(config_path) as f:
            json.load(f)
    except (json.JSONDecodeError, OSError) as exc:
        raise ConversionError(
            f"config.json is not valid JSON: {exc}"
        ) from exc
