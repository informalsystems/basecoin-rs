.PHONY: test create-channel token-transfer upgrade-client grpc-service recover-client

test: create-channel token-transfer upgrade-client grpc-service recover-client
	@echo "All tests passed"

create-channel:
	@echo "Creating channel..."
	bash ~/tests/create-channel.sh
	@echo "Channel created"

token-transfer: upgrade-client
	@echo "Transferring tokens..."
	bash ~/tests/token-transfer.sh
	@echo "Tokens transferred"

upgrade-client: create-channel
	@echo "Upgrading client..."
	bash ~/tests/upgrade-client.sh
	@echo "Client upgraded"

recover-client: upgrade-client
	@echo "Recovering client..."
	bash ~/tests/recover-client.sh
	@echo "Client recovered"

token-transfer: create-channel recover-client upgrade-client
	@echo "Transferring tokens..."
	bash ~/tests/token-transfer.sh
	@echo "Tokens transferred"

grpc-service: token-transfer
	@echo "Testing gRPC services..."
	bash ~/tests/grpc-service.sh
	@echo "gRPC services working"
