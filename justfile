serverdir := "dummy-folder/server"
clientdir := "dummy-folder/client"

# start server
start-server:
    cargo run -- server

# start client
start-client:
    cargo run -- client client.toml

# setup tests
setup: setup-dirs create-rand-file

# setup dummy folders for testing
setup-dirs:
    rm -rf dummy-folder/
    mkdir -p {{serverdir}} {{clientdir}}
    echo '*' > dummy-folder/.gitignore

# create random file on client
create-rand-file:
    openssl rand -out {{clientdir}}/foo-$(openssl rand -hex 8).mrc 128
