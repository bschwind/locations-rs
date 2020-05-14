//! Handlers for `/city/*` endpoints.

use crate::{
    response::{ErrorResponse, ErrorResponse::BadRequest, JsonResult},
    services::locations_repo::{ElasticCity, Language, LocationsElasticRepository},
    stateful::elasticsearch::WithElastic,
    AppState,
};
use actix_web::web::{Data, Json, Query};
use futures::{stream::FuturesOrdered, TryStreamExt};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;

/// Query for the `/city/v1/get` endpoint.
#[derive(Deserialize)]
pub(crate) struct CityQuery {
    /// Id of the city to get, positive integer.
    id: u64,
    language: Language,
}

/// `City` API entity. All city endpoints respond with this payload (or a composition of it).
#[allow(non_snake_case)]
#[derive(Serialize)]
pub(crate) struct CityResponse {
    /// Id of the city, e.g. `123`.
    id: u64,
    /// Whether this city is marked as *featured*, e.g. `false`.
    isFeatured: bool,
    /// ISO 3166-1 alpha-2 country code, or a custom 4-letter code, e.g. `"CZ"`.
    countryIso: String,
    /// E.g. `"Plzeň"`.
    name: String,
    /// E.g. `"Plzeňský kraj"`.
    regionName: String,
}

/// The `/city/v1/get` endpoint. HTTP request: [`CityQuery`], response: [`CityResponse`].
///
/// Get city of given ID localized to given language.
pub(crate) async fn get(query: Query<CityQuery>, app: Data<AppState>) -> JsonResult<CityResponse> {
    let locations_es_repo = LocationsElasticRepository(app.get_ref());
    let es_city = locations_es_repo.get_city(query.id).await?;

    Ok(Json(es_city.into_resp(app.get_ref(), query.language).await?))
}

/// Query for the `/city/v1/featured` endpoint.
#[derive(Deserialize)]
pub(crate) struct FeaturedQuery {
    language: Language,
}

/// A list of `City` API entities.
#[derive(Serialize)]
pub(crate) struct MultiCityResponse {
    cities: Vec<CityResponse>,
}

/// The `/city/v1/featured` endpoint. HTTP request: [`FeaturedQuery`], response: [`MultiCityResponse`].
///
/// Returns a list of all featured cities.
pub(crate) async fn featured(
    query: Query<FeaturedQuery>,
    app: Data<AppState>,
) -> JsonResult<MultiCityResponse> {
    let locations_es_repo = LocationsElasticRepository(app.get_ref());
    let mut es_cities = locations_es_repo.get_featured_cities().await?;

    let preferred_country_iso = match query.language {
        Language::CS => "CZ",
        Language::DE => "DE",
        Language::EN => "CZ",
        Language::PL => "PL",
        Language::SK => "SK",
    };
    es_cities.sort_by_key(|c| Reverse(c.countryIso == preferred_country_iso));

    es_cities_into_resp(app.get_ref(), es_cities, query.language).await
}

/// Query for the `/city/v1/search` endpoint.
#[allow(non_snake_case)]
#[derive(Deserialize)]
pub(crate) struct SearchQuery {
    /// The search query.
    query: String,
    /// ISO 3166-1 alpha-2 country code. Can be used to limit scope of the search to a given country.
    countryIso: Option<String>,
    language: Language,
}

/// The `/city/v1/search` endpoint. HTTP request: [`SearchQuery`], response: [`MultiCityResponse`].
///
/// Returns list of cities matching the 'query' parameter.
/// The response is limited to 10 cities and no pagination is provided.
pub(crate) async fn search(
    query: Query<SearchQuery>,
    app: Data<AppState>,
) -> JsonResult<MultiCityResponse> {
    let locations_es_repo = LocationsElasticRepository(app.get_ref());
    let es_cities =
        locations_es_repo.search(&query.query, query.language, query.countryIso.as_deref()).await?;

    es_cities_into_resp(app.get_ref(), es_cities, query.language).await
}

impl ElasticCity {
    /// Transform ElasticCity into CityResponse, fetching the region.
    async fn into_resp<T: WithElastic>(
        self,
        app: &T,
        language: Language,
    ) -> Result<CityResponse, ErrorResponse> {
        let locations_es_repo = LocationsElasticRepository(app);
        let es_region = locations_es_repo.get_region(self.regionId).await?;

        let name_key = language.name_key();
        let name = self.names.get(&name_key).ok_or_else(|| BadRequest(name_key.clone()))?;
        let region_name = es_region.names.get(&name_key).ok_or_else(|| BadRequest(name_key))?;

        Ok(CityResponse {
            id: self.id,
            isFeatured: self.isFeatured,
            countryIso: self.countryIso,
            name: name.to_string(),
            regionName: region_name.to_string(),
        })
    }
}

/// Convert a vector of [ElasticCity] into [MultiCityResponse], maintaining order and fetching
/// required regions asynchronously all in parallel (which is somewhat redundant with
/// [ElasticRegion] cache).
async fn es_cities_into_resp<T: WithElastic>(
    app: &T,
    es_cities: Vec<ElasticCity>,
    language: Language,
) -> JsonResult<MultiCityResponse> {
    let city_futures: FuturesOrdered<_> =
        es_cities.into_iter().map(|it| it.into_resp(app, language)).collect();

    city_futures.try_collect().await.map(|cities| Json(MultiCityResponse { cities }))
}
