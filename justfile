serverdir := "dummy-folder/server"
clientdir := "dummy-folder/client"

# start server
start-server:
    cargo run -- server start dummy-folder/server.toml

# start client
start-client:
    cargo run -- client start dummy-folder/client.toml

# setup tests
setup:
    rm -rf dummy-folder/
    mkdir -p {{serverdir}} {{clientdir}}
    echo '*' > dummy-folder/.gitignore
    just create-rand-file
    just create-rand-file
    cargo run -- client config dummy-folder/client.toml
    cargo run -- server config dummy-folder/server.toml

# create random file on client
create-rand-file:
    openssl rand -out {{clientdir}}/foo-$(openssl rand -hex 8).mrc 128

# prepare a new release
release version:
    @if [ -n "$(git status --porcelain || echo "dirty")" ]; then echo "repo is dirty!"; exit 1; fi
    sed -i 's/^version = ".*"$/version = "{{ version }}"/g' Cargo.toml
    git add Cargo.toml
    cargo check
    git add Cargo.lock
    git commit -m "release {{ version }}"
    git tag -m "Release {{ version }}" -a -e "v{{ version }}"
    @echo "check last commit and amend as necessary, then git push --follow-tags"
