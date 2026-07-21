use std::env;
use std::path::Path;

fn main() {
    // Obtém o diretório raiz de onde o Cargo está rodando (onde o build.rs e o .rlib devem estar)
    let dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    
    // Instrui o Cargo/Rustc a procurar por dependências (.rlib) neste diretório
    println!("cargo:rustc-link-search=dependency={}", Path::new(&dir).display());
    
    // (Opcional) Recompila se o arquivo da biblioteca for atualizado
    println!("cargo:rerun-if-changed=libaxolote.rlib");
}
