FROM rust

WORKDIR /application

COPY ./ ./

RUN cargo build --release

# Expose the application port
EXPOSE 5000

# Set the command to run the application
CMD [ "./target/release/bc_labs-assignment" ]
