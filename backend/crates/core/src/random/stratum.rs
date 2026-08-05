use crate::error::{RandomizationError, RandomizationResult};
use crate::random::RandomizationRequest;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Definition of a single stratification factor for a study.
///
/// Points to a value within the request's FHIR resource context that
/// partitions subjects into subgroups for balanced allocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stratum {
    /// Unique identifier for this stratum definition.
    pub id: String,
    /// Human-readable label.
    pub name: String,
    /// FHIR resource type to look up in the request context (e.g. `"Patient"`).
    pub context: String,
    /// JSON pointer path within the resource (e.g. `"/gender"`).
    pub path: String,
}

/// A resolved stratification value extracted from a request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SimpleStratum {
    /// A value extracted from a FHIR resource path.
    FhirPath {
        /// Stratum identifier (matches [`Stratum::id`]).
        name: String,
        /// Extracted value for this subject.
        value: String,
    },
}

/// One or more resolved stratification values.
///
/// [`Simple`](Self::Simple) when a study has a single stratum;
/// [`Composite`](Self::Composite) when multiple strata are combined.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum Strata {
    /// A single resolved stratum.
    Simple(SimpleStratum),
    /// Multiple resolved strata combined as a composite key.
    Composite(Vec<SimpleStratum>),
}

impl SimpleStratum {
    /// Extract a stratum value from the request context.
    ///
    /// # Errors
    ///
    /// Return [`RandomizationError::NotFound`] if the request has no context,
    /// the context lacks the specified resource, or the path does not resolve.
    pub fn try_parse(
        stratum: &Stratum,
        request: &RandomizationRequest,
    ) -> RandomizationResult<Self> {
        let request_context = request
            .context
            .as_ref()
            .ok_or(RandomizationError::NotFound)?;

        let context = stratum.context.as_str();
        let path = stratum.path.as_str();

        let resource = request_context
            .get(context)
            .ok_or(RandomizationError::NotFound)?;
        let value = get_by_pointer(resource, path).ok_or(RandomizationError::NotFound)?;

        Ok(SimpleStratum::FhirPath {
            name: stratum.id.clone(),
            value,
        })
    }
}

impl Strata {
    /// Parse all strata definitions against the request context.
    ///
    /// # Errors
    ///
    /// Return [`RandomizationError::NotFound`] if the slice is empty or
    /// any individual stratum cannot be resolved from the request.
    pub fn try_parse(
        strata: &[Stratum],
        request: &RandomizationRequest,
    ) -> RandomizationResult<Self> {
        match strata.len() {
            0 => Err(RandomizationError::NotFound),
            1 => Ok(Strata::Simple(SimpleStratum::try_parse(
                &strata[0], request,
            )?)),
            _ => {
                let strata = strata
                    .iter()
                    .map(|s| SimpleStratum::try_parse(s, request))
                    .collect::<RandomizationResult<Vec<SimpleStratum>>>()?;
                Ok(Strata::Composite(strata))
            }
        }
    }
}

impl From<&SimpleStratum> for String {
    fn from(stratum: &SimpleStratum) -> Self {
        match stratum {
            SimpleStratum::FhirPath { name, value } => format!("{}={}", name, value),
        }
    }
}

impl From<&Strata> for String {
    fn from(strata: &Strata) -> Self {
        match strata {
            Strata::Simple(stratum) => stratum.into(),
            Strata::Composite(strata) => strata
                .iter()
                .map(|s| s.into())
                .collect::<Vec<String>>()
                .join("|"),
        }
    }
}

fn get_by_pointer<T>(v: &Value, pointer: &str) -> Option<T>
where
    T: DeserializeOwned,
{
    v.pointer(pointer)
        .and_then(|val| serde_json::from_value(val.clone()).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request_with_context() -> RandomizationRequest {
        RandomizationRequest {
            locator: None,
            patient_id: None,
            encounter_id: None,
            user_id: None,
            context: Some(json!({
                "Patient": {
                    "gender": "male",
                    "age": "45"
                }
            })),
        }
    }

    fn age_stratum() -> Stratum {
        Stratum {
            id: "age".to_string(),
            name: "Age".to_string(),
            context: "Patient".to_string(),
            path: "/age".to_string(),
        }
    }

    #[test]
    fn simple_stratum_try_parse_extracts_value_from_context() {
        let stratum = age_stratum();
        let request = request_with_context();

        let result = SimpleStratum::try_parse(&stratum, &request).unwrap();

        assert!(matches!(
            result,
            SimpleStratum::FhirPath { ref name, ref value } if name == "age" && value == "45"
        ));
    }

    #[test]
    fn simple_stratum_try_parse_returns_not_found_when_context_missing() {
        let stratum = age_stratum();
        let request = RandomizationRequest {
            locator: None,
            patient_id: None,
            encounter_id: None,
            user_id: None,
            context: None,
        };

        let result = SimpleStratum::try_parse(&stratum, &request);

        assert!(matches!(result.unwrap_err(), RandomizationError::NotFound));
    }

    #[test]
    fn simple_stratum_try_parse_returns_not_found_when_path_invalid() {
        let stratum = Stratum {
            id: "x".to_string(),
            name: "X".to_string(),
            context: "Patient".to_string(),
            path: "/nonexistent".to_string(),
        };
        let request = request_with_context();

        let result = SimpleStratum::try_parse(&stratum, &request);

        assert!(matches!(result.unwrap_err(), RandomizationError::NotFound));
    }

    #[test]
    fn strata_try_parse_returns_not_found_for_empty_slice() {
        let request = request_with_context();

        let result = Strata::try_parse(&[], &request);

        assert!(matches!(result.unwrap_err(), RandomizationError::NotFound));
    }

    #[test]
    fn strata_try_parse_returns_simple_for_single_stratum() {
        let strata = vec![age_stratum()];
        let request = request_with_context();

        let result = Strata::try_parse(&strata, &request).unwrap();

        assert!(matches!(result, Strata::Simple(_)));
    }

    #[test]
    fn strata_try_parse_returns_composite_for_multiple_strata() {
        let strata = vec![
            age_stratum(),
            Stratum {
                id: "gender".to_string(),
                name: "Gender".to_string(),
                context: "Patient".to_string(),
                path: "/gender".to_string(),
            },
        ];
        let request = request_with_context();

        let result = Strata::try_parse(&strata, &request).unwrap();

        assert!(matches!(result, Strata::Composite(ref v) if v.len() == 2));
    }
}
