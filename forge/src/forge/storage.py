"""OpenDAL-based model storage operations for converted ONNX artifacts."""

from __future__ import annotations

import os

import opendal

from forge.config import ForgeSettings


def build_operator(settings: ForgeSettings) -> opendal.Operator:
    """Construct an OpenDAL Operator from ForgeSettings fields.

    The Operator's ``root`` incorporates the storage prefix so that
    callers never need to prepend prefix paths manually.
    """
    kwargs: dict[str, str] = {}

    if settings.storage_bucket:
        kwargs["bucket"] = settings.storage_bucket

    if settings.storage_region:
        kwargs["region"] = settings.storage_region

    if settings.storage_type == "fs":
        if settings.storage_root:
            root = settings.storage_root
            if settings.storage_prefix:
                root = f"{root}/{settings.storage_prefix}"
            kwargs["root"] = root
    else:
        if settings.storage_prefix:
            kwargs["root"] = f"/{settings.storage_prefix}"

    return opendal.Operator(settings.storage_type, **kwargs)


def upload_to_storage(
    op: opendal.Operator,
    model_id: str,
    local_dir: str,
) -> list[str]:
    """Upload all files from *local_dir* to storage (recursive).

    Walks *local_dir* recursively so that files in subdirectories
    (e.g., ``onnx/model.onnx``) are included.  Storage path format:
    ``{model_id}/{relative_path}``.  The Operator root already
    includes any configured prefix, so no prefix parameter is needed.

    Returns the list of uploaded storage paths.
    """
    uploaded: list[str] = []

    for root, _dirs, files in os.walk(local_dir):
        for filename in sorted(files):
            filepath = os.path.join(root, filename)
            relative = os.path.relpath(filepath, local_dir)
            path = f"{model_id}/{relative}"
            with open(filepath, "rb") as f:
                op.write(path, f.read())
            uploaded.append(path)

    return uploaded
