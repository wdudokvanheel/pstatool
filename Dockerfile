# First stage: Build the Rust application
FROM rust:1.84 as builder

LABEL authors="Wesley Dudok van Heel"

WORKDIR /app
COPY . .
RUN cargo build --release

# Second stage: Use official Nginx image and install cloc
FROM nginx:stable

# Install cloc
RUN apt-get update && apt-get install -y cloc && rm -rf /var/lib/apt/lists/
COPY --from=builder /app/target/release/pstatool /usr/local/bin/pstatool

RUN printf 'expires 6h;\nadd_header Cache-Control "public, max-age=21600, must-revalidate";\n' \
      > /etc/nginx/conf.d/cache-control.conf

EXPOSE 80

RUN mkdir /tmp/pstatool
RUN rm /usr/share/nginx/html/*

COPY assets/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
