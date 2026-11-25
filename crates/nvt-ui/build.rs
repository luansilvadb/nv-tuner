//! Ícone (ID 1, usado também pelo GPUI na janela e na barra de tarefas) e
//! VERSIONINFO do nvt-ui.exe.

include!("../../assets/recurso-win.rs");

fn main() {
    embutir_recurso_win("nv-tuner", "nvt-ui");
}
