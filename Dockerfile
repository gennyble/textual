FROM rust:1.63.0 as build-env
WORKDIR /app
COPY . /app
RUN cargo build --release

FROM gcr.io/distroless/cc
COPY --from=build-env /app/target/release/textualimagery /
COPY --from=build-env /app/textual.conf /
COPY --from=build-env /app/*.html /
ENTRYPOINT ["./textualimagery", "-c", "/textual.conf", "--font-cache", "/fonts", "-l", "0.0.0.0"]
