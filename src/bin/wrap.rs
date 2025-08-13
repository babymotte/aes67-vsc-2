fn main() {
    let mut counter: u32 = 38;
    let inc = 48;

    loop {
        let new_val = counter.wrapping_add(inc);
        if new_val < counter {
            eprintln!("{new_val}");
        }
        counter = new_val;
    }
}
