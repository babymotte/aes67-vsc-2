# Aes67Vsc2 Dockerfile for x86_64
#
# Copyright (C) 2024 Michael Bachmann
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU Affero General Public License for more details.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.

FROM lukemathwalker/cargo-chef:latest-rust-1 AS aes67-vsc-2-chef
WORKDIR /app

FROM aes67-vsc-2-chef AS aes67-vsc-2-planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM aes67-vsc-2-chef AS aes67-vsc-2-builder 
COPY --from=aes67-vsc-2-planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build -p aes67-vsc-2 --release

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=aes67-vsc-2-builder /app/target/release/aes67-vsc-2 /usr/local/bin
ENV RUST_LOG=info
ENTRYPOINT ["/usr/local/bin/aes67-vsc-2"]
