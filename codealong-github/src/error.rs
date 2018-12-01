use codealong;
use git2;

error_chain! {
    foreign_links {
        Git2(git2::Error);
        IO(std::io::Error);
    }

    links {
        Core(codealong::Error, codealong::ErrorKind);
    }
}
