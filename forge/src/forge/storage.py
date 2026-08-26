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
    """Upload all files from *local_dir* to S3.

    S3 key format matches the Hephaestus resolver layout:
    ``{prefix}/{model_id}/{filename}`` when *prefix* is non-empty,
    ``{model_id}/{filename}`` otherwise.

    Returns the list of uploaded S3 keys.
    """
    s3 = boto3.client("s3")
    config = TransferConfig(
        multipart_threshold=100 * 1024 * 1024,  # 100 MB
        max_concurrency=4,
    )

    uploaded_keys: list[str] = []
    for filename in sorted(os.listdir(local_dir)):
        filepath = os.path.join(local_dir, filename)
        if not os.path.isfile(filepath):
            continue
        if prefix:
            s3_key = f"{prefix}/{model_id}/{filename}"
        else:
            s3_key = f"{model_id}/{filename}"
        s3.upload_file(filepath, bucket, s3_key, Config=config)
        uploaded_keys.append(s3_key)

    return uploaded_keys
