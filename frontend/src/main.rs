use leptos::*;

mod app;
mod components;
mod core;
mod pages;

fn main() {
    mount_to_body(app::App);
}
