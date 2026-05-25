use w16_cc::W16C;

fn main() {
    let code = "unsigned main() { return 67; }";

    W16C::execute_code_by_vm(&W16C::new(code));
}