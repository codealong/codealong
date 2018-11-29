use codealong;
use codealong_elk;

error_chain! {

    foreign_links {
        IO(std::io::Error);
    }

    links {
        Core(codealong::Error, codealong::ErrorKind);
        Elk(codealong_elk::Error, codealong_elk::ErrorKind);
    }
}
