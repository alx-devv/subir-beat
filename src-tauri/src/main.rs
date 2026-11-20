// Sin ventana de consola en Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    subir_beat_lib::run()
}
