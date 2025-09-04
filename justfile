serverdir := "dummy-folder/server"
clientdir := "dummy-folder/client"

# start server
[working-directory: 'dummy-folder']
start-server:
    cargo run -- server server.toml

# start client
[working-directory: 'dummy-folder']
start-client:
    cargo run -- client client.toml

# setup tests
setup:
    rm -rf dummy-folder/
    mkdir -p {{serverdir}} {{clientdir}}
    echo '*' > dummy-folder/.gitignore
    just create-rand-file
    just create-rand-file
    cargo run -- print-config client dummy-folder/client.toml
    cargo run -- print-config server dummy-folder/server.toml

# create random file on client
create-rand-file:
    openssl rand -out {{clientdir}}/foo-$(openssl rand -hex 8).mrc 128

# prepare a new release
release version:
    @if [ -n "$(git status --porcelain || echo "dirty")" ]; then echo "repo is dirty!"; exit 1; fi
    sed -i 's/^version = ".*"$/version = "{{ version }}"/g' Cargo.toml
    git add Cargo.toml
    git commit -m "release {{ version }}"
    git tag -m "Release {{ version }}" -a -e "v{{ version }}"
    @echo "check last commit and amend as necessary, then git push --follow-tags"
