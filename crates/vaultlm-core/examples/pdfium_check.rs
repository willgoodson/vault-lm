use pdfium_render::prelude::*;
fn main() {
    let dir = std::env::args().nth(1).unwrap();
    let lib = Pdfium::pdfium_platform_library_name_at_path(&dir);
    println!("lib: {} exists={}", lib.display(), lib.exists());
    match Pdfium::bind_to_library(&lib) { Ok(_) => println!("bind ok"), Err(e) => println!("bind error: {e:?}") }
}
