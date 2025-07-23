serverdir := "dummy-folder/server"
clientdir := "dummy-folder/client"

# start server
start-server:
    cargo run -- server dummy-folder/server.toml

# start client
start-client:
    cargo run -- client dummy-folder/client.toml

# setup tests
setup:
    rm -rf dummy-folder/
    mkdir -p {{serverdir}} {{clientdir}}
    echo '*' > dummy-folder/.gitignore
    just create-rand-file
    just create-rand-file
    cargo run -- print-config client > dummy-folder/client.toml
    cargo run -- print-config server > dummy-folder/server.toml

# create random file on client
create-rand-file:
    openssl rand -out {{clientdir}}/foo-$(openssl rand -hex 8).mrc 128
