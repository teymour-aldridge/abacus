use abacus::config::make_rocket;
use rocket::launch;

#[launch]
async fn launch() -> _ {
    make_rocket()
}
