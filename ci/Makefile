.PHONY: test create-channel token-transfer upgrade-client

test: create-channel token-transfer upgrade-client

create-channel:
	@echo "Creating channel..."
	bash ~/tests/create-channel.sh
	@echo "Channel created"

token-transfer: create-channel
	@echo "Transferring tokens..."
	bash ~/tests/token-transfer.sh
	@echo "Tokens transferred"

upgrade-client: create-channel
	@echo "Upgrading client..."
	bash ~/tests/upgrade-client.sh
	@echo "Client upgraded"
