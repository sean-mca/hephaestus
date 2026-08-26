"""S3 upload logic for converted ONNX model artifacts."""

from __future__ import annotations

import os

import boto3
from boto3.s3.transfer import TransferConfig


def upload_to_s3(
    local_dir: str,
    bucket: str,
    prefix: str,
    model_id: str,
) -> list[str]:
    """Upload all files from *local_dir* to S3 (recursive).

    Walks *local_dir* recursively so that files in subdirectories
    (e.g., ``onnx/model.onnx``) are included.  S3 key format matches
    the Hephaestus resolver layout:
    ``{prefix}/{model_id}/{relative_path}`` when *prefix* is non-empty,
    ``{model_id}/{relative_path}`` otherwise.

    Returns the list of uploaded S3 keys.
    """
    s3 = boto3.client("s3")
    config = TransferConfig(
        multipart_threshold=100 * 1024 * 1024,  # 100 MB
        max_concurrency=4,
    )

    uploaded_keys: list[str] = []
    for root, _dirs, files in os.walk(local_dir):
        for filename in sorted(files):
            filepath = os.path.join(root, filename)
            relative = os.path.relpath(filepath, local_dir)
            if prefix:
                s3_key = f"{prefix}/{model_id}/{relative}"
            else:
                s3_key = f"{model_id}/{relative}"
            s3.upload_file(filepath, bucket, s3_key, Config=config)
            uploaded_keys.append(s3_key)

    return uploaded_keys
