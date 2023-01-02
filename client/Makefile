.PHONY: venv
venv:
	python3 -m venv venv

.PHONY: install-deps
install-deps: venv
	. ./venv/bin/activate && pip install -r requirements.txt

.PHONY: generate-protos
generate-protos: install-deps
	mkdir -p protos
	protoc -I=../protos --python_out=protos ../protos/kv.proto
