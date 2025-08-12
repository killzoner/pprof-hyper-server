FROM rust:bookworm

COPY Makefile .

RUN set -eux && \
  apt update && \
  apt install --yes --no-install-recommends ca-certificates unzip make && \
  echo "deb http://deb.debian.org/debian/ bookworm main non-free" | tee /etc/apt/sources.list && \
  rustup component add clippy && \
  rm Makefile && \
  cargo install taplo-cli@0.9.3 && \
  cargo install cargo-machete && \
  rustup component add rustfmt && \
  rm -rf /var/lib/apt/lists/* && \
  export version=24.0.6 && \
  curl --silent --fail --location https://download.docker.com/linux/static/stable/x86_64/docker-${version}.tgz | tar --extract --gzip --directory=/usr/local/bin/ --strip-components 1 && \
  git config --global --add safe.directory /workdir

CMD su ${USER} --shell /bin/bash --session-command "export HOME=${WORKDIR} && /bin/bash"
