.PHONY: test create-channel recover-client token-transfer upgrade-client grpc-service

test: create-channel recover-client token-transfer upgrade-client grpc-service
	@echo "All tests passed"

create-channel:
	@echo "Creating channel..."
	bash ~/tests/create-channel.sh
	@echo "Channel created"

recover-client: create-channel
	@echo "Recovering client..."
	bash ~/tests/recover-client.sh
	@echo "Client recovered"

token-transfer: create-channel
	@echo "Transferring tokens..."
	bash ~/tests/token-transfer.sh
	@echo "Tokens transferred"

upgrade-client: create-channel
	@echo "Upgrading client..."
	bash ~/tests/upgrade-client.sh
	@echo "Client upgraded"

grpc-service: upgrade-client
	@echo "Testing gRPC services..."
	bash ~/tests/grpc-service.sh
	@echo "gRPC services working"
