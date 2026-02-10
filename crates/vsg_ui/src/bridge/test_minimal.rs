#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qproperty(i32, number)]
        type MyObject = super::MyObjectRust;

        #[qinvokable]
        fn say_hi(self: Pin<&mut MyObject>, name: &QString);
    }
}

pub struct MyObjectRust {
    number: i32,
}

impl Default for MyObjectRust {
    fn default() -> Self {
        Self { number: 42 }
    }
}

impl qobject::MyObject {
    fn say_hi(self: std::pin::Pin<&mut Self>, name: &qobject::QString) {
        println!("Hi {}!", name.to_string());
    }
}
