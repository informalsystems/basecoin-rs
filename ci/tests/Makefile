all: create-channel token-transfer upgrade-client

.PHONY: create-channel token-transfer upgrade-client

create-channel:
	@echo "Creating channel..."
	bash create-channel.sh
	@echo "Channel created"

token-transfer: create-channel
	@echo "Transferring tokens..."
	bash token-transfer.sh
	@echo "Tokens transferred"

upgrade-client: create-channel
	@echo "Upgrading client..."
	bash upgrade-client.sh
	@echo "Client upgraded"
