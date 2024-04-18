.PHONY: test create-channel token-transfer upgrade-client grpc-service recover-client

# test: create-channel token-transfer upgrade-client grpc-service recover-client
test: create-channel token-transfer upgrade-client recover-client
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

# grpc-service: upgrade-client
# 	@echo "Testing gRPC services..."
# 	bash ~/tests/grpc-service.sh
# 	@echo "gRPC services working"

recover-client: create-channel
	@echo "Recovering client..."
	base ~/tests/recover-client.sh
	@echo "Client recovered"
