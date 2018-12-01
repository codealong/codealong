use codealong;
use codealong_elk;
use codealong_github;

error_chain! {

    foreign_links {
        IO(std::io::Error);
    }

    links {
        Core(codealong::Error, codealong::ErrorKind);
        Elk(codealong_elk::Error, codealong_elk::ErrorKind);
        Github(codealong_github::Error, codealong_github::ErrorKind);
    }
}
