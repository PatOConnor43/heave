POST {{ baseurl }}
Authorization: Bearer {{ authorization }}
```graphql
query {
  fallibleListPets {
    ... on PetsResponse {
      pets {
        id
        name
        tag
        friend {
          id
          name
          tag
          friend {
            id
            name
            tag
            friend {
              id
              name
              tag
            }
          }
        }
      }
      nextPage
    }
    ... on Error {
      code
      message
    }
  }
}
```
HTTP 200
