.PHONY: build test lint fmt up down logs multi gc swagger hygiene

# Build everything (all services share one workspace)
build:
	cargo build --workspace

test:
	cargo test --workspace

lint:
	cargo clippy --workspace --all-targets

fmt:
	cargo fmt --all

# Docker compose stack (preview)
up:
	docker compose up -d --build

down:
	docker compose down

logs:
	docker compose logs -f --tail 100

# Start the second polyphony instance (parallel backfill / swap-over demo)
multi:
	docker compose --profile multi up -d --build polyphony-b

# Manual one-shot TiKV MVCC garbage collection (the tikv-gc service also runs
# automatically every 10 minutes)
gc:
	docker compose run --rm tikv-gc tikv-gc

# Regenerate the committed OpenAPI document
swagger:
	cargo run -p mapi -- docs

# Check for internal references that must not ship
hygiene:
	@! grep -rniE "@gomaestro\.org|pkg\.dev|svc\.cluster\.local|maestro-org-development|ssh://|DEPLOY_KEY|haproxy-dataplane|dotswap|magic.?eden" \
		--include="*.rs" --include="*.toml" --include="*.md" --include="*.yml" --include="*.yaml" --include="Dockerfile" --include="*.sh" \
		--exclude-dir=target . \
		| grep -v "^./Makefile" || (echo "hygiene check failed" && exit 1)
	@echo "hygiene check passed"
