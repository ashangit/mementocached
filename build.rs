fn main() {
    protobuf_codegen::Codegen::new()
        .protoc()
        .includes(["protos"])
        .input("protos/kv.proto")
        .cargo_out_dir("protos")
        .run_from_script();
}
