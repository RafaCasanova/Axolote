# Backend Axolote — Makefile
# Framework de Servidor HTTP nativo em Rust (Sem Cargo)

LIB_SRC = src/lib.rs
LIB_NAME = axolote
LIB_OUT = lib$(LIB_NAME).rlib

# Lista de testes HTTP
HTTP_TESTS = test_server

# Lista de exemplos HTTP
HTTP_EXAMPLES = 01_basic_routing 02_parameters 03_groups_and_middleware 04_advanced_config 05_http_logger 06_standalone_logger basic_routing groups_middleware query_parameters

# Lista de testes WebSocket
WS_TESTS = 08_test_broadcast 10_test_cluster 11_test_gossip_mesh test_passive_client test_active_client test_pm_server test_pm_bob test_pm_eve test_pm_alice cluster_simple cluster_complex 13_security_extreme_test

# Lista de exemplos WebSocket
WS_EXAMPLES = 07_websocket 12_secure_handshake 14_secure_ws broadcast_chat private_chat

TESTS = $(HTTP_TESTS) $(WS_TESTS)
EXAMPLES = $(HTTP_EXAMPLES) $(WS_EXAMPLES)

.PHONY: all build-lib tests examples clean $(TESTS) $(EXAMPLES)

all: tests examples

## Compila o framework como biblioteca estática (.rlib)
build-lib:
	@echo "Construindo biblioteca do framework..."
	rustc -g --crate-type=lib $(LIB_SRC) --crate-name $(LIB_NAME) -o $(LIB_OUT)

## Compila todos os testes e exemplos
tests: build-lib $(TESTS)
examples: build-lib $(EXAMPLES)

## Regras para compilar testes HTTP
$(HTTP_TESTS): build-lib
	@echo "Compilando teste HTTP $@..."
	rustc -g tests/http/$@.rs --extern $(LIB_NAME)=$(LIB_OUT) -o $@

## Regras para compilar exemplos HTTP
$(HTTP_EXAMPLES): build-lib
	@echo "Compilando exemplo HTTP $@..."
	rustc -g docs/http/examples/$@.rs --extern $(LIB_NAME)=$(LIB_OUT) -o $@

## Regras para compilar testes WebSocket
$(WS_TESTS): build-lib
	@echo "Compilando teste WebSocket $@..."
	rustc -g tests/websocket/$@.rs --extern $(LIB_NAME)=$(LIB_OUT) -o $@

## Regras para compilar exemplos WebSocket
$(WS_EXAMPLES): build-lib
	@echo "Compilando exemplo WebSocket $@..."
	rustc -g docs/websocket/examples/$@.rs --extern $(LIB_NAME)=$(LIB_OUT) -o $@

## Limpa os binários e bibliotecas
clean:
	@echo "Limpando artefatos compilados..."
	rm -f $(LIB_OUT) $(TESTS) $(EXAMPLES)
	rm -f *.o *.d
