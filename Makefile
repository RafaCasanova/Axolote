# Backend Axolote — Makefile
# Framework de Servidor HTTP nativo em Rust (Sem Cargo)

LIB_SRC = src/lib.rs
LIB_NAME = axolote
LIB_OUT = lib$(LIB_NAME).rlib

MACRO_SRC = axolote_macros/src/lib.rs
MACRO_NAME = axolote_macros
MACRO_OUT = lib$(MACRO_NAME).so

# Lista de testes HTTP
HTTP_TESTS = test_server

# Lista de exemplos HTTP
HTTP_EXAMPLES = hello_world path_and_query_params routes_grouping_middleware server_advanced_config http_request_logger background_task_logger json_rest_api multipart_form_data basic_routing groups_middleware query_parameters

# Lista de testes WebSocket
WS_TESTS = test_room_broadcast test_cluster_s2s test_cluster_gossip_mesh test_passive_client test_active_client test_pm_server test_pm_bob test_pm_eve test_pm_alice cluster_simple cluster_complex test_security_extreme

# Lista de exemplos WebSocket
WS_EXAMPLES = basic_websocket secure_handshake_auth websocket_token_auth broadcast_chat private_chat custom_connection_id

TESTS = $(HTTP_TESTS) $(WS_TESTS)
EXAMPLES = $(HTTP_EXAMPLES) $(WS_EXAMPLES)

.PHONY: all build-lib tests examples clean $(TESTS) $(EXAMPLES)

all: tests examples

## Compila a procedural macro (.so)
build-macros:
	@echo "Construindo procedural macros..."
	rustc -g --crate-type=proc-macro $(MACRO_SRC) --crate-name $(MACRO_NAME) -o $(MACRO_OUT)

## Compila o framework como biblioteca estática (.rlib)
build-lib: build-macros
	@echo "Construindo biblioteca do framework..."
	rustc -g --crate-type=lib $(LIB_SRC) --crate-name $(LIB_NAME) --extern $(MACRO_NAME)=$(MACRO_OUT) -o $(LIB_OUT)

## Compila todos os testes e exemplos
tests: build-lib $(TESTS)
examples: build-lib $(EXAMPLES)

## Regras para compilar testes HTTP
$(HTTP_TESTS): build-lib
	@echo "Compilando teste HTTP $@..."
	rustc -L . -g tests/http/$@.rs --extern axolote=libaxolote.rlib -o $@

## Regras para compilar exemplos HTTP
$(HTTP_EXAMPLES): build-lib
	@echo "Compilando exemplo HTTP $@..."
	rustc -L . -g docs/http/examples/$@.rs --extern axolote=libaxolote.rlib -o $@

## Regras para compilar testes WebSocket
$(WS_TESTS): build-lib
	@echo "Compilando teste WebSocket $@..."
	rustc -L . -g tests/websocket/$@.rs --extern axolote=libaxolote.rlib -o $@

## Regras para compilar exemplos WebSocket
$(WS_EXAMPLES): build-lib
	@echo "Compilando exemplo WebSocket $@..."
	rustc -L . -g docs/websocket/examples/$@.rs --extern axolote=libaxolote.rlib -o $@

## Limpa os binários e bibliotecas
clean:
	@echo "Limpando artefatos compilados..."
	rm -f $(LIB_OUT) $(MACRO_OUT) $(TESTS) $(EXAMPLES)
	rm -f *.o *.d
	@find . -maxdepth 1 -type f -executable -not -name "*.*" -delete

