// Example test to verify SentenceTransformer integration
// This would typically go in tests/ directory or within the module

#[cfg(test)]
mod embedding_tests {
    use crate::config::Config;
    use crate::embedding::{EmbeddingProviderType, create_embedding_provider_from_parts};
    use crate::embedding::types::{parse_provider_model, EmbeddingConfig};

    #[test]
    fn test_sentence_transformer_provider_creation() {
        // Test that we can create a SentenceTransformer provider
        let provider_type = EmbeddingProviderType::SentenceTransformer;
        let model = "sentence-transformers/all-MiniLM-L6-v2";
        
        let result = create_embedding_provider_from_parts(&provider_type, model);
        assert!(result.is_ok(), "Should be able to create SentenceTransformer provider");
    }

    #[test]
    fn test_provider_model_parsing() {
        // Test the new provider:model syntax parsing
        let test_cases = vec![
            ("sentencetransformer:sentence-transformers/all-MiniLM-L6-v2", 
             EmbeddingProviderType::SentenceTransformer, 
             "sentence-transformers/all-MiniLM-L6-v2"),
            ("fastembed:all-MiniLM-L6-v2", 
             EmbeddingProviderType::FastEmbed, 
             "all-MiniLM-L6-v2"),
            ("jinaai:jina-embeddings-v3", 
             EmbeddingProviderType::Jina, 
             "jina-embeddings-v3"),
            ("all-MiniLM-L6-v2", // Legacy format without provider
             EmbeddingProviderType::FastEmbed, 
             "all-MiniLM-L6-v2"),
        ];

        for (input, expected_provider, expected_model) in test_cases {
            let (provider, model) = parse_provider_model(input);
            assert_eq!(provider, expected_provider, "Provider should match for input: {}", input);
            assert_eq!(model, expected_model, "Model should match for input: {}", input);
        }
    }

    #[test]
    fn test_default_config_format() {
        // Test that default config uses new provider:model format
        let config = Config::default();
        
        // Check that default models use provider:model format
        assert!(config.embedding.code_model.contains(':'), 
                "Code model should use provider:model format");
        assert!(config.embedding.text_model.contains(':'), 
                "Text model should use provider:model format");
        
        // Test parsing the default models
        let (code_provider, _) = parse_provider_model(&config.embedding.code_model);
        let (text_provider, _) = parse_provider_model(&config.embedding.text_model);
        assert_eq!(code_provider, EmbeddingProviderType::FastEmbed);
        assert_eq!(text_provider, EmbeddingProviderType::FastEmbed);
    }

    #[test]
    fn test_embedding_config_methods() {
        let config = EmbeddingConfig {
            code_model: "sentencetransformer:microsoft/codebert-base".to_string(),
            text_model: "sentencetransformer:sentence-transformers/all-mpnet-base-v2".to_string(),
            jina: Default::default(),
            voyage: Default::default(),
            google: Default::default(),
        };
        
        // Test getting active provider
        let active_provider = config.get_active_provider();
        assert_eq!(active_provider, EmbeddingProviderType::SentenceTransformer);
        
        // Test vector dimensions
        let dim = config.get_vector_dimension(&EmbeddingProviderType::SentenceTransformer, "microsoft/codebert-base");
        assert_eq!(dim, 768);
        
        let dim2 = config.get_vector_dimension(&EmbeddingProviderType::SentenceTransformer, "sentence-transformers/all-MiniLM-L6-v2");
        assert_eq!(dim2, 384);
    }

    // Note: This test would require network access and is more of an integration test
    // #[tokio::test]
    // async fn test_sentence_transformer_embedding_generation() {
    //     let mut config = Config::default();
    //     config.embedding.provider = EmbeddingProviderType::SentenceTransformer;
    //     config.embedding.sentencetransformer.text_model = 
    //         "sentencetransformer:sentence-transformers/all-MiniLM-L6-v2".to_string();
    //     
    //     let text = "This is a test text for embedding generation.";
    //     let result = generate_embeddings(text, false, &config).await;
    //     
    //     assert!(result.is_ok(), "Should generate embeddings successfully");
    //     let embeddings = result.unwrap();
    //     assert_eq!(embeddings.len(), 384, "all-MiniLM-L6-v2 should produce 384-dimensional embeddings");
    //     
    //     // Verify embeddings are normalized (L2 norm should be approximately 1.0)
    //     let norm: f32 = embeddings.iter().map(|x| x * x).sum::<f32>().sqrt();
    //     assert!((norm - 1.0).abs() < 0.01, "Embeddings should be normalized");
    // }
}