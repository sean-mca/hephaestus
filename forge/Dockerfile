# ---------- builder ----------
FROM python:3.13-slim AS builder

COPY --from=ghcr.io/astral-sh/uv:latest /uv /uvx /bin/

WORKDIR /app

# Install dependencies first for layer caching.
COPY pyproject.toml uv.lock ./
RUN uv sync --frozen --no-dev --no-install-project 2>/dev/null \
    || uv sync --no-dev --no-install-project

# Copy source and install the project itself.
COPY src/ src/
RUN uv sync --frozen --no-dev 2>/dev/null \
    || uv sync --no-dev

# Strip bytecode caches and pip metadata to shrink the venv.
RUN find /app/.venv -type d -name __pycache__ -exec rm -rf {} + 2>/dev/null; \
    find /app/.venv -name '*.pyc' -delete 2>/dev/null; \
    rm -rf /app/.venv/lib/python*/site-packages/pip /app/.venv/lib/python*/site-packages/setuptools; \
    true

# ---------- runtime ----------
FROM python:3.13-slim

RUN useradd -r -m -s /bin/false forge

WORKDIR /app

COPY --from=builder --chown=forge:forge /app/.venv /app/.venv
COPY --from=builder --chown=forge:forge /app/src /app/src

USER forge

ENV PATH="/app/.venv/bin:$PATH"
ENV PYTHONPATH="/app/src"
ENV PORT=8080
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD python -c "import urllib.request; urllib.request.urlopen('http://localhost:8080/health')" || exit 1

CMD ["uvicorn", "forge.main:create_app", "--factory", "--host", "0.0.0.0", "--port", "8080"]
