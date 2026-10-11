#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Validation(String),
    #[error("Ce compagnon n'existe plus.")]
    ProfileNotFound,
    #[error("Ce profil a changé. Recharge-le avant de l'enregistrer.")]
    Conflict,
    #[error("Un événement d'une ancienne conversation a été ignoré.")]
    StaleOperation,
    #[error("Il faut conserver au moins un compagnon.")]
    LastProfile,
    #[error("La base a été créée par une version plus récente de Sherka.")]
    FutureSchema,
    #[error("Erreur de stockage : {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Impossible d'accéder aux fichiers : {0}")]
    Io(#[from] std::io::Error),
    #[error("Le format de configuration n'est pas valide : {0}")]
    Json(#[from] serde_json::Error),
    #[error("L'image PNG ou WebP n'est pas lisible : {0}")]
    Image(#[from] image::ImageError),
}

pub type Result<T> = std::result::Result<T, Error>;
