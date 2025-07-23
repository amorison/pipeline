serverdir := "dummy-folder/server"
clientdir := "dummy-folder/client"

# start server
start-server:
    cargo run -- server server.toml

# start client
start-client:
    cargo run -- client client.toml

# setup tests
setup:
    rm -rf dummy-folder/
    mkdir -p {{serverdir}} {{clientdir}}
    echo '*' > dummy-folder/.gitignore
    just create-rand-file
    just create-rand-file

# create random file on client
create-rand-file:
    openssl rand -out {{clientdir}}/foo-$(openssl rand -hex 8).mrc 128
