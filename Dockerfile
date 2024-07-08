# syntax=docker/dockerfile:1

FROM lukemathwalker/cargo-chef:latest-rust-alpine AS chef

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /recipe.json .
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release

FROM docker:cli AS runtime
COPY --from=builder /target/release/autopilot /usr/local/bin/autopilot

WORKDIR /usr/local/share/autopilot
ENV APP_HOST=0.0.0.0
ENV APP_PORT=8000
EXPOSE 8000
CMD ["/usr/local/bin/autopilot"]
