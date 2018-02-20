.PHONY: default build test doc clean

CARGO_FLAGS := --features "$(FEATURES)"

default: test

build:
	cargo build $(CARGO_FLAGS)

test: build
	cargo test $(CARGO_FLAGS)
	cd examples/chat && cargo build

skeptic:
	USE_SKEPTIC=1 cargo test $(CARGO_FLAGS)

# cd examples/word-count && python setup.py install && pytest -v tests

clippy:
	if $$CLIPPY; then cargo clippy $(CARGO_FLAGS); fi

doc: build
	cargo doc --no-deps $(CARGO_FLAGS)
	cd guide; mdbook build -d ../target/doc/guide/; cd ..

clean:
	rm -r target

.PHONY: gh-pages-doc
gh-pages-doc: doc
	cd gh-pages && git pull
	rm -r gh-pages/doc
	cp -r target/doc gh-pages/
	rm gh-pages/doc/.lock
	cd gh-pages && git add .
	cd gh-pages && git commit -m "Update documentation"

publish: default gh-pages-doc
	cargo publish
	cd gh-pages && git push
