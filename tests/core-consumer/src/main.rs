use term_squid_core::ucum::UcumProvider;
fn main() {
    assert!(UcumProvider::lookup("mg/dL", Some("2.2"))
        .unwrap()
        .is_some());
    assert!(UcumProvider::lookup("not-a-unit", None).unwrap().is_none());
    assert!(UcumProvider::comparable("mg/dL", "g/L").unwrap());
    assert!(!UcumProvider::comparable("m", "s").unwrap());
    assert!(UcumProvider::lookup("m", Some("missing")).is_err());
}
