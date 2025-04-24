#[cxx::bridge(namespace = "f1x::aasdk")]
pub mod ffi {
    #[namespace = "f1x::aasdk::common"]
    unsafe extern "C++" {
        include!("f1x/aasdk/Common/Data.hpp");
        type DataBuffer;
        type DataConstBuffer;
    }
    #[namespace = "f1x::aasdk::channel::bluetooth"]
    unsafe extern "C++" {
        include!("f1x/aasdk/Channel/Bluetooth/BluetoothServiceChannel.hpp");
        type BluetoothServiceChannel;
    }
}